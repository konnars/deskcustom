use anyhow::{Context, Result};

pub fn read_text() -> Result<Option<String>> {
    let mut cb = arboard::Clipboard::new().context("open clipboard")?;
    match cb.get_text() {
        Ok(text) if text.is_empty() => Ok(None),
        Ok(text) => Ok(Some(text)),
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

pub fn write_text(text: &str) -> Result<()> {
    let mut cb = arboard::Clipboard::new().context("open clipboard")?;
    cb.set_text(text).context("set clipboard text")?;
    Ok(())
}
