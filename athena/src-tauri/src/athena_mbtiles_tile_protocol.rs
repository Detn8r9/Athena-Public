//! Custom `athena-mbtiles` URI scheme: serves individual raster tiles from MBTiles archives under the application data `maps/` directory.
//!
//! Request path shape: `/{file}.mbtiles/{z}/{x}/{y}` where `z`, `x`, and `y` use the standard XYZ (slippy map) convention.

use std::path::PathBuf;

use tauri::http::header::{self, HeaderValue};
use tauri::http::{Method, Request, Response, StatusCode};
use tauri::{AppHandle, Manager, Runtime};

use crate::mbtiles_io;

/// Registered URI scheme name (lowercase, no colon).
pub const ATHENA_MBTILES_SCHEME: &str = "athena-mbtiles";

const MAPS_DIRECTORY_NAME: &str = "maps";

/// Chains [`tauri::Builder::register_asynchronous_uri_scheme_protocol`] for [`ATHENA_MBTILES_SCHEME`].
pub fn register_with_builder<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.register_asynchronous_uri_scheme_protocol(
        ATHENA_MBTILES_SCHEME,
        |context, request, responder| {
            let application_handle = context.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                let response = match tauri::async_runtime::spawn_blocking(move || {
                    process_request(&application_handle, request)
                })
                .await
                {
                    Ok(resp) => resp,
                    Err(_) => fallback_error_response(),
                };
                responder.respond(response);
            });
        },
    )
}

fn process_request<R: Runtime>(
    application_handle: &AppHandle<R>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    match request.method() {
        m if m == Method::OPTIONS => options_response(),
        m if m == Method::HEAD => serve_head(application_handle, &request),
        m if m == Method::GET => serve_get(application_handle, &request),
        _ => plain_text_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "Only GET, HEAD, and OPTIONS are supported for this resource.",
        ),
    }
}

fn options_response() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        )
        .header(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, HEAD, OPTIONS"),
        )
        .header(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Range, Content-Type, Accept"),
        )
        .body(Vec::new())
        .unwrap_or_else(|_| fallback_error_response())
}

fn serve_head<R: Runtime>(
    application_handle: &AppHandle<R>,
    request: &Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    match resolve_tile_request(application_handle, request.uri().path()) {
        Ok((file_path, _, _, _)) => {
            let content_type = match mbtiles_io::read_mbtiles_raster_format_mime(&file_path) {
                Ok(mime) => mime,
                Err(message) => {
                    return plain_text_response(StatusCode::UNSUPPORTED_MEDIA_TYPE, &message);
                }
            };
            Response::builder()
                .status(StatusCode::OK)
                .header(
                    header::ACCESS_CONTROL_ALLOW_ORIGIN,
                    HeaderValue::from_static("*"),
                )
                .header(header::CONTENT_TYPE, HeaderValue::from_static(content_type))
                .body(Vec::new())
                .unwrap_or_else(|_| fallback_error_response())
        }
        Err(response) => response,
    }
}

fn serve_get<R: Runtime>(
    application_handle: &AppHandle<R>,
    request: &Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    match resolve_tile_request(application_handle, request.uri().path()) {
        Ok((file_path, z, x, y_xyz)) => {
            let content_type = match mbtiles_io::read_mbtiles_raster_format_mime(&file_path) {
                Ok(mime) => mime,
                Err(message) => {
                    return plain_text_response(StatusCode::UNSUPPORTED_MEDIA_TYPE, &message);
                }
            };

            match mbtiles_io::read_mbtiles_raster_tile_xyz(&file_path, z, x, y_xyz) {
                Ok(Some(bytes)) => Response::builder()
                    .status(StatusCode::OK)
                    .header(
                        header::ACCESS_CONTROL_ALLOW_ORIGIN,
                        HeaderValue::from_static("*"),
                    )
                    .header(header::CONTENT_TYPE, HeaderValue::from_static(content_type))
                    .header(
                        header::CACHE_CONTROL,
                        HeaderValue::from_static("public, max-age=86400"),
                    )
                    .header(header::CONTENT_LENGTH, header_value_from_usize(bytes.len()))
                    .body(bytes)
                    .unwrap_or_else(|_| fallback_error_response()),
                Ok(None) => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .header(
                        header::ACCESS_CONTROL_ALLOW_ORIGIN,
                        HeaderValue::from_static("*"),
                    )
                    .header(
                        header::CONTENT_TYPE,
                        HeaderValue::from_static("text/plain; charset=utf-8"),
                    )
                    .body(b"Tile not found in this MBTiles archive.".to_vec())
                    .unwrap_or_else(|_| fallback_error_response()),
                Err(message) => plain_text_response(StatusCode::INTERNAL_SERVER_ERROR, &message),
            }
        }
        Err(response) => response,
    }
}

fn resolve_tile_request<R: Runtime>(
    application_handle: &AppHandle<R>,
    uri_path: &str,
) -> Result<(PathBuf, u32, u32, u32), Response<Vec<u8>>> {
    let application_data_directory = match application_handle.path().app_data_dir() {
        Ok(directory) => directory,
        Err(error) => {
            return Err(plain_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Could not resolve application data directory: {error}"),
            ));
        }
    };

    let maps_directory = application_data_directory.join(MAPS_DIRECTORY_NAME);
    let canonical_maps_root = match maps_directory.canonicalize() {
        Ok(path) => path,
        Err(_) => {
            return Err(plain_text_response(
                StatusCode::NOT_FOUND,
                "The maps directory does not exist yet. Download a map archive first.",
            ));
        }
    };

    let segments: Vec<&str> = uri_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.len() != 4 {
        return Err(plain_text_response(
            StatusCode::BAD_REQUEST,
            "Expected path /{archive}.mbtiles/{z}/{x}/{y}.",
        ));
    }

    let archive_segment_encoded = segments[0];
    let zoom_segment = segments[1];
    let column_segment = segments[2];
    let row_segment = segments[3];

    let archive_file_name = match urlencoding::decode(archive_segment_encoded) {
        Ok(value) => value.into_owned(),
        Err(decode_error) => {
            return Err(plain_text_response(
                StatusCode::BAD_REQUEST,
                &format!("Invalid percent-encoding in the archive name: {decode_error}"),
            ));
        }
    };

    if archive_file_name.contains("..") || archive_file_name.contains('/') {
        return Err(plain_text_response(
            StatusCode::FORBIDDEN,
            "Archive name must be a single file in the maps directory.",
        ));
    }

    let lower_archive = archive_file_name.to_ascii_lowercase();
    if !lower_archive.ends_with(".mbtiles") {
        return Err(plain_text_response(
            StatusCode::FORBIDDEN,
            "Only files ending with .mbtiles may be accessed through this tile protocol.",
        ));
    }

    let z: u32 = zoom_segment.parse().map_err(|_| {
        plain_text_response(
            StatusCode::BAD_REQUEST,
            "Zoom level must be a non-negative integer.",
        )
    })?;
    let x: u32 = column_segment.parse().map_err(|_| {
        plain_text_response(
            StatusCode::BAD_REQUEST,
            "Tile X index must be a non-negative integer.",
        )
    })?;
    let y_xyz: u32 = row_segment.parse().map_err(|_| {
        plain_text_response(
            StatusCode::BAD_REQUEST,
            "Tile Y index must be a non-negative integer.",
        )
    })?;

    let candidate_path = canonical_maps_root.join(&archive_file_name);
    let canonical_file = match candidate_path.canonicalize() {
        Ok(path) => path,
        Err(_) => {
            return Err(plain_text_response(
                StatusCode::NOT_FOUND,
                "The requested MBTiles archive was not found in the maps directory.",
            ));
        }
    };

    if !canonical_file.starts_with(&canonical_maps_root) {
        return Err(plain_text_response(
            StatusCode::FORBIDDEN,
            "Resolved path escapes the maps directory.",
        ));
    }

    Ok((canonical_file, z, x, y_xyz))
}

fn header_value_from_usize(value: usize) -> HeaderValue {
    HeaderValue::from_str(&value.to_string()).unwrap_or_else(|_| HeaderValue::from_static("0"))
}

fn plain_text_response(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    let body = message.as_bytes().to_vec();
    Response::builder()
        .status(status)
        .header(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        )
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        )
        .header(header::CONTENT_LENGTH, header_value_from_usize(body.len()))
        .body(body)
        .unwrap_or_else(|_| fallback_error_response())
}

fn fallback_error_response() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        )
        .body(b"Internal server error while building the HTTP response.".to_vec())
        .unwrap()
}
