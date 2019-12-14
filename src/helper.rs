use crate::error::{Error, Result};
use std::path::Path;

/// Construct a linker function for unix systems.
#[cfg(unix)]
pub fn get_link_function<P, Q>() -> impl FnMut(P, Q) -> Result<()>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    |src, dst| {
        std::os::unix::fs::symlink(src.as_ref(), dst.as_ref()).map_err(|e| {
            let src_string = src.as_ref().to_string_lossy().into();
            let dst_string = dst.as_ref().to_string_lossy().into();
            Error::FailedToCreateTargetLink(src_string, dst_string, e)
        })
    }
}

/// Construct a linker function for windows systems.
#[cfg(windows)]
pub fn get_link_function<P, Q>() -> impl FnMut(P, Q) -> Result<()>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    |src, dst| {
        std::os::windows::fs::symlink_file(src, dst).map_err(|e| {
            let src_string = src.as_ref().to_string_lossy().into();
            let dst_string = dst.as_ref().to_string_lossy().into();
            Error::FailedToCreateTargetLink(src_string, dst_string, e)
        })
    }
}
