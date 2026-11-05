//! Thin, safe conversions around raw `libc` return conventions.

use std::io;

/// Converts a `-1 + errno` style return value into an [`io::Result`].
pub(crate) fn cvt(ret: libc::c_int) -> io::Result<libc::c_int> {
    if ret == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret)
    }
}

/// Converts a "returns the error number directly" value (e.g. `posix_fadvise`).
pub(crate) fn cvt_errno(ret: libc::c_int) -> io::Result<()> {
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(ret))
    }
}

/// Converts a byte offset to the platform's signed offset type.
pub(crate) fn to_off<T: TryFrom<u64>>(value: u64) -> io::Result<T> {
    T::try_from(value)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "offset out of range"))
}

/// `true` when an error means "this filesystem/kernel does not support that call".
///
/// Optimisation hints use this to fail soft instead of aborting the copy.
pub fn is_unsupported(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::Unsupported
        || matches!(
            err.raw_os_error(),
            Some(libc::EOPNOTSUPP | libc::ENOTTY | libc::ENOSYS | libc::EINVAL | libc::ESPIPE)
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cvt_passes_through_success() {
        assert_eq!(cvt(3).unwrap(), 3);
    }

    #[test]
    fn cvt_errno_maps_code() {
        let err = cvt_errno(libc::EBADF).unwrap_err();
        assert_eq!(err.raw_os_error(), Some(libc::EBADF));
    }

    #[test]
    fn to_off_rejects_out_of_range() {
        assert!(to_off::<i64>(u64::MAX).is_err());
    }

    #[test]
    fn eopnotsupp_is_unsupported() {
        assert!(is_unsupported(&io::Error::from_raw_os_error(
            libc::EOPNOTSUPP
        )));
    }

    #[test]
    fn enospc_is_a_real_error() {
        assert!(!is_unsupported(&io::Error::from_raw_os_error(libc::ENOSPC)));
    }
}
