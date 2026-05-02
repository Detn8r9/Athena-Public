//! Custom `athena-map` URI scheme for read-only access to offline map archives under the application data `maps/` directory.
//!
//! Web requests use platform-specific URL forms (see Tauri documentation for custom protocols). This handler resolves the path segment after the host, validates it stays inside `maps/`, and serves only `.pmtiles` and `.mbtiles` files.
//!
//! Large archives are served with HTTP range requests. Full-file responses without a `Range` header are only allowed up to [`MAX_FULL_RESPONSE_BYTES`] so the handler does not load multi-gigabyte files into memory at once.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use tauri::http::header::{self, HeaderValue};
use tauri::http::{Method, Request, Response, StatusCode};
use tauri::{AppHandle, Runtime};

use crate::mbtiles_io;
use crate::pmtiles_io;
use crate::portable;

/// Registered URI scheme name (lowercase, no colon).
pub const ATHENA_MAP_SCHEME: &str = "athena-map";

const MAPS_DIRECTORY_NAME: &str = "maps";
const MAX_FULL_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_SINGLE_RANGE_BYTES: u64 = 8 * 1024 * 1024;

/// Chains [`tauri::Builder::register_asynchronous_uri_scheme_protocol`] for [`ATHENA_MAP_SCHEME`].
pub fn register_with_builder<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.register_asynchronous_uri_scheme_protocol(
        ATHENA_MAP_SCHEME,
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
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            HeaderValue::from_static("Accept-Ranges, Content-Length, Content-Range, Content-Type"),
        )
        .body(Vec::new())
        .unwrap_or_else(|_| fallback_error_response())
}

fn serve_head<R: Runtime>(
    application_handle: &AppHandle<R>,
    request: &Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let uri_path = request.uri().path();
    if let Some((archive_path, _, _, _, is_pmtiles)) =
        try_resolve_raster_tile_request(application_handle, uri_path)
    {
        let content_type = if is_pmtiles {
            "image/png"
        } else {
            match mbtiles_io::read_mbtiles_raster_format_mime(&archive_path) {
                Ok(mime) => mime,
                Err(message) => {
                    return plain_text_response(StatusCode::UNSUPPORTED_MEDIA_TYPE, &message);
                }
            }
        };
        return Response::builder()
            .status(StatusCode::OK)
            .header(
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            )
            .header(header::CONTENT_TYPE, HeaderValue::from_static(content_type))
            .body(Vec::new())
            .unwrap_or_else(|_| fallback_error_response());
    }

    let resolved_path = match resolve_local_archive_path(application_handle, uri_path) {
        Ok(path) => path,
        Err(response) => return response,
    };

    let metadata = match std::fs::metadata(&resolved_path) {
        Ok(meta) => meta,
        Err(error) => {
            return plain_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Could not read file metadata: {error}"),
            );
        }
    };

    if !metadata.is_file() {
        return plain_text_response(StatusCode::NOT_FOUND, "The requested path is not a file.");
    }

    let file_length = metadata.len();
    let content_type = content_type_for_archive_path(&resolved_path);

    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        )
        .header(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"))
        .header(header::CONTENT_TYPE, HeaderValue::from_static(content_type))
        .header(header::CONTENT_LENGTH, header_value_from_u64(file_length))
        .body(Vec::new())
        .unwrap_or_else(|_| fallback_error_response())
}

fn serve_get<R: Runtime>(
    application_handle: &AppHandle<R>,
    request: &Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let uri_path = request.uri().path();
    if let Some((archive_path, z, x, y, is_pmtiles)) =
        try_resolve_raster_tile_request(application_handle, uri_path)
    {
        let content_type = if is_pmtiles {
            "image/png"
        } else {
            match mbtiles_io::read_mbtiles_raster_format_mime(&archive_path) {
                Ok(mime) => mime,
                Err(message) => {
                    return plain_text_response(StatusCode::UNSUPPORTED_MEDIA_TYPE, &message);
                }
            }
        };
        let tile_result = if is_pmtiles {
            pmtiles_io::read_pmtiles_raster_tile_xyz(&archive_path, z, x, y)
        } else {
            mbtiles_io::read_mbtiles_raster_tile_xyz(&archive_path, z, x, y)
        };
        return match tile_result {
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
                .header(header::CONTENT_LENGTH, header_value_from_u64(bytes.len() as u64))
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
                .body(b"Tile not found in this archive.".to_vec())
                .unwrap_or_else(|_| fallback_error_response()),
            Err(message) => plain_text_response(StatusCode::INTERNAL_SERVER_ERROR, &message),
        };
    }

    let resolved_path = match resolve_local_archive_path(application_handle, uri_path) {
        Ok(path) => path,
        Err(response) => return response,
    };

    let mut file = match File::open(&resolved_path) {
        Ok(opened) => opened,
        Err(error) => {
            return plain_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Could not open archive file: {error}"),
            );
        }
    };

    let metadata = match file.metadata() {
        Ok(meta) => meta,
        Err(error) => {
            return plain_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Could not read file metadata: {error}"),
            );
        }
    };

    if !metadata.is_file() {
        return plain_text_response(StatusCode::NOT_FOUND, "The requested path is not a file.");
    }

    let file_length = metadata.len();
    let content_type = content_type_for_archive_path(&resolved_path);

    if let Some(range_value) = request.headers().get(header::RANGE) {
        let range_header_text = match range_value.to_str() {
            Ok(text) => text,
            Err(_) => {
                return range_not_satisfiable_response(file_length);
            }
        };

        match parse_single_byte_range(range_header_text, file_length) {
            Ok((range_start, range_end_inclusive)) => {
                let byte_count = range_end_inclusive
                    .saturating_sub(range_start)
                    .saturating_add(1);

                if file.seek(SeekFrom::Start(range_start)).is_err() {
                    return plain_text_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Could not seek to the requested byte range in the archive file.",
                    );
                }

                let mut buffer = vec![0u8; byte_count as usize];
                if file.read_exact(&mut buffer).is_err() {
                    return plain_text_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Could not read the requested byte range from the archive file.",
                    );
                }

                let content_range_value =
                    format!("bytes {range_start}-{range_end_inclusive}/{}", file_length);

                Response::builder()
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header(
                        header::ACCESS_CONTROL_ALLOW_ORIGIN,
                        HeaderValue::from_static("*"),
                    )
                    .header(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"))
                    .header(header::CONTENT_TYPE, HeaderValue::from_static(content_type))
                    .header(header::CONTENT_LENGTH, header_value_from_u64(byte_count))
                    .header(
                        header::CONTENT_RANGE,
                        HeaderValue::from_str(&content_range_value)
                            .unwrap_or_else(|_| HeaderValue::from_static("bytes */0")),
                    )
                    .body(buffer)
                    .unwrap_or_else(|_| fallback_error_response())
            }
            Err(()) => range_not_satisfiable_response(file_length),
        }
    } else if file_length > MAX_FULL_RESPONSE_BYTES {
        let response = plain_text_response(
            StatusCode::FORBIDDEN,
            "This archive is too large to download without a Range header. Clients must issue ranged GET requests.",
        );
        let (mut parts, body) = response.into_parts();
        parts
            .headers
            .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
        parts
            .headers
            .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        parts
            .headers
            .insert(header::CONTENT_LENGTH, header_value_from_u64(file_length));
        Response::from_parts(parts, body)
    } else {
        let mut buffer = vec![0u8; file_length as usize];
        if file.read_exact(&mut buffer).is_err() {
            return plain_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not read the full archive file from disk.",
            );
        }

        Response::builder()
            .status(StatusCode::OK)
            .header(
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            )
            .header(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"))
            .header(header::CONTENT_TYPE, HeaderValue::from_static(content_type))
            .header(header::CONTENT_LENGTH, header_value_from_u64(file_length))
            .body(buffer)
            .unwrap_or_else(|_| fallback_error_response())
    }
}

fn range_not_satisfiable_response(file_length: u64) -> Response<Vec<u8>> {
    let content_range_value = format!("bytes */{file_length}");
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        )
        .header(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&content_range_value)
                .unwrap_or_else(|_| HeaderValue::from_static("bytes */0")),
        )
        .header(header::CONTENT_LENGTH, HeaderValue::from_static("0"))
        .body(Vec::new())
        .unwrap_or_else(|_| fallback_error_response())
}

fn parse_single_byte_range(header_text: &str, file_length: u64) -> Result<(u64, u64), ()> {
    if file_length == 0 {
        return Err(());
    }

    let trimmed = header_text.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("bytes=") {
        return Err(());
    }

    let range_spec = trimmed.get(6..).ok_or(())?.trim();
    let first_range = range_spec.split(',').next().ok_or(())?.trim();
    if first_range.is_empty() {
        return Err(());
    }

    let dash_index = first_range.find('-').ok_or(())?;
    let left = first_range.get(..dash_index).ok_or(())?;
    let right = first_range.get(dash_index + 1..).ok_or(())?;

    let last_byte_index = file_length.saturating_sub(1);

    let (start, mut end_inclusive) = if left.is_empty() {
        let suffix_length: u64 = right.parse().map_err(|_| ())?;
        if suffix_length == 0 {
            return Err(());
        }
        if suffix_length >= file_length {
            (0u64, last_byte_index)
        } else {
            let start_byte = file_length.saturating_sub(suffix_length);
            (start_byte, last_byte_index)
        }
    } else {
        let start_byte: u64 = left.parse().map_err(|_| ())?;
        let end_byte: u64 = if right.is_empty() {
            last_byte_index
        } else {
            right.parse().map_err(|_| ())?
        };

        let end_clamped = end_byte.min(last_byte_index);
        if start_byte > end_clamped || start_byte >= file_length {
            return Err(());
        }
        (start_byte, end_clamped)
    };

    if start > end_inclusive {
        return Err(());
    }

    let span = end_inclusive.saturating_sub(start).saturating_add(1);
    if span > MAX_SINGLE_RANGE_BYTES {
        end_inclusive = start
            .saturating_add(MAX_SINGLE_RANGE_BYTES)
            .saturating_sub(1);
    }

    if start >= file_length {
        return Err(());
    }

    end_inclusive = end_inclusive.min(last_byte_index);
    if end_inclusive < start {
        return Err(());
    }

    Ok((start, end_inclusive))
}

#[allow(clippy::result_large_err)]
fn canonical_maps_root<R: Runtime>(
    application_handle: &AppHandle<R>,
) -> Result<std::path::PathBuf, Response<Vec<u8>>> {
    let application_data_directory = match portable::resolved_app_data_dir(application_handle) {
        Ok(directory) => directory,
        Err(error) => {
            return Err(plain_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Could not resolve application data directory: {error}"),
            ));
        }
    };

    let maps_directory = application_data_directory.join(MAPS_DIRECTORY_NAME);
    match maps_directory.canonicalize() {
        Ok(path) => Ok(path),
        Err(_) => Err(plain_text_response(
            StatusCode::NOT_FOUND,
            "The maps directory does not exist yet. Download a map archive first.",
        )),
    }
}

/// `/{archive}.mbtiles|pmtiles/{z}/{x}/{y}` under the maps directory (XYZ).
fn try_resolve_raster_tile_request<R: Runtime>(
    application_handle: &AppHandle<R>,
    uri_path: &str,
) -> Option<(PathBuf, u32, u32, u32, bool)> {
    let canonical_maps_root = canonical_maps_root(application_handle).ok()?;
    let segments: Vec<&str> = uri_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.len() != 4 {
        return None;
    }
    let archive_segment_encoded = segments[0];
    let zoom_segment = segments[1];
    let column_segment = segments[2];
    let row_segment = segments[3];

    let archive_file_name = urlencoding::decode(archive_segment_encoded).ok()?.into_owned();
    if archive_file_name.contains("..") || archive_file_name.contains('/') {
        return None;
    }
    let lower = archive_file_name.to_ascii_lowercase();
    let is_pmtiles = lower.ends_with(".pmtiles");
    let is_mbtiles = lower.ends_with(".mbtiles");
    if !is_pmtiles && !is_mbtiles {
        return None;
    }

    let z: u32 = zoom_segment.parse().ok()?;
    let x: u32 = column_segment.parse().ok()?;
    let y_xyz: u32 = row_segment.parse().ok()?;

    let candidate_path = canonical_maps_root.join(&archive_file_name);
    let canonical_file = candidate_path.canonicalize().ok()?;
    if !canonical_file.starts_with(&canonical_maps_root) {
        return None;
    }
    Some((canonical_file, z, x, y_xyz, is_pmtiles))
}

#[allow(clippy::result_large_err)]
fn resolve_local_archive_path<R: Runtime>(
    application_handle: &AppHandle<R>,
    uri_path: &str,
) -> Result<PathBuf, Response<Vec<u8>>> {
    let canonical_maps_root = canonical_maps_root(application_handle)?;

    let trimmed_path = uri_path.trim_start_matches('/');
    if trimmed_path.is_empty() || trimmed_path.contains("..") || trimmed_path.contains('/') {
        return Err(plain_text_response(
            StatusCode::FORBIDDEN,
            "Only a single file name in the maps directory is allowed.",
        ));
    }

    let decoded_segment = match urlencoding::decode(trimmed_path) {
        Ok(decoded) => decoded.into_owned(),
        Err(decode_error) => {
            return Err(plain_text_response(
                StatusCode::BAD_REQUEST,
                &format!("Invalid percent-encoding in the request path: {decode_error}"),
            ));
        }
    };

    let lower_name = decoded_segment.to_ascii_lowercase();
    if !lower_name.ends_with(".pmtiles") && !lower_name.ends_with(".mbtiles") {
        return Err(plain_text_response(
            StatusCode::FORBIDDEN,
            "Only files ending with .pmtiles or .mbtiles may be accessed through this protocol.",
        ));
    }

    let candidate_path = canonical_maps_root.join(&decoded_segment);
    let canonical_file = match candidate_path.canonicalize() {
        Ok(path) => path,
        Err(_) => {
            return Err(plain_text_response(
                StatusCode::NOT_FOUND,
                "The requested archive file was not found in the maps directory.",
            ));
        }
    };

    if !canonical_file.starts_with(&canonical_maps_root) {
        return Err(plain_text_response(
            StatusCode::FORBIDDEN,
            "Resolved path escapes the maps directory.",
        ));
    }

    Ok(canonical_file)
}

fn content_type_for_archive_path(path: &Path) -> &'static str {
    let name = path.to_string_lossy().to_lowercase();
    if name.ends_with(".pmtiles") {
        "application/vnd.pmtiles"
    } else if name.ends_with(".mbtiles") {
        "application/vnd.sqlite3"
    } else {
        "application/octet-stream"
    }
}

fn header_value_from_u64(value: u64) -> HeaderValue {
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
        .header(
            header::CONTENT_LENGTH,
            header_value_from_u64(body.len() as u64),
        )
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

#[cfg(test)]
mod tests {
    use super::{parse_single_byte_range, MAX_SINGLE_RANGE_BYTES};

    /// Phase 2: a client asks for 100 MiB in one Range; the handler must clamp to 8 MiB per range.
    #[test]
    fn byte_range_span_is_capped_at_eight_megabytes() {
        const HUNDRED_MIB: u64 = 100 * 1024 * 1024;
        let file_length: u64 = 256 * 1024 * 1024;
        let last_index_for_100mib = HUNDRED_MIB - 1;
        let header = format!("bytes=0-{last_index_for_100mib}");

        let (start, end_inclusive) =
            parse_single_byte_range(&header, file_length).expect("valid range");
        assert_eq!(start, 0);
        let span = end_inclusive.saturating_sub(start).saturating_add(1);
        assert_eq!(span, MAX_SINGLE_RANGE_BYTES);
        assert_eq!(
            end_inclusive,
            start
                .saturating_add(MAX_SINGLE_RANGE_BYTES)
                .saturating_sub(1)
        );
    }
}
