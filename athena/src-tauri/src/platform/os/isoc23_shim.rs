//! Ubuntu 22.04 (glibc 2.35) does not export the C23 `__isoc23_strto*` symbols.
//! Some prebuilt native libraries (notably C++) may reference these directly.
//! Defining them here keeps our Linux build linkable on glibc < 2.38.

#[cfg(target_os = "linux")]
mod shim {
    use std::ffi::c_char;

    // Some prebuilt ONNX Runtime binaries use the LLVM libc++ ABI on Linux/aarch64.
    // Ensure the linker pulls in the correct C++ ABI libs.
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    #[link(name = "c++")]
    #[link(name = "c++abi")]
    #[link(name = "supc++")]
    extern "C" {}

    #[no_mangle]
    pub unsafe extern "C" fn __isoc23_strtol(
        nptr: *const c_char,
        endptr: *mut *mut c_char,
        base: i32,
    ) -> libc::c_long {
        libc::strtol(nptr, endptr, base)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __isoc23_strtoll(
        nptr: *const c_char,
        endptr: *mut *mut c_char,
        base: i32,
    ) -> libc::c_longlong {
        libc::strtoll(nptr, endptr, base)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __isoc23_strtoul(
        nptr: *const c_char,
        endptr: *mut *mut c_char,
        base: i32,
    ) -> libc::c_ulong {
        libc::strtoul(nptr, endptr, base)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __isoc23_strtoull(
        nptr: *const c_char,
        endptr: *mut *mut c_char,
        base: i32,
    ) -> libc::c_ulonglong {
        libc::strtoull(nptr, endptr, base)
    }
}

