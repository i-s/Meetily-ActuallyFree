//! Lightweight media identity for revalidation, not a content hash/retention lease.
use std::{fs, path::Path};

pub(crate) fn revision(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("Saved playback track is not a file".into());
    }
    let revision = format!(
        "{}:{:?}",
        metadata.len(),
        metadata.modified().map_err(|e| e.to_string())?
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return Ok(format!(
            "{revision}:{}:{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            metadata.ctime(),
            metadata.ctime_nsec()
        ));
    }
    #[cfg(not(unix))]
    Ok(revision)
}
