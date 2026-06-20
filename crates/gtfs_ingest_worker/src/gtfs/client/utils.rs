use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn temp_output_path(output_file: &Path) -> Result<PathBuf> {
    let file_name = output_file
        .file_name()
        .with_context(|| {
            format!(
                "failed to get file name from output file path {}",
                output_file.display()
            )
        })?
        .to_string_lossy();

    Ok(output_file.with_file_name(format!(".{file_name}.tmp")))
}

pub fn replace_file(temp_file: &Path, output_file: &Path) -> anyhow::Result<()> {
    match std::fs::rename(temp_file, output_file) {
        Ok(()) => Ok(()),
        Err(error) if output_file.exists() => {
            std::fs::remove_file(output_file).with_context(|| {
                format!("failed to remove previous file {}", output_file.display())
            })?;
            std::fs::rename(temp_file, output_file).with_context(|| {
                format!(
                    "failed to move file from {} to {} after removing previous output: {}",
                    temp_file.display(),
                    output_file.display(),
                    error
                )
            })
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to move file from {} to {}",
                temp_file.display(),
                output_file.display()
            )
        }),
    }
}
