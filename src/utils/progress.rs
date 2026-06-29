use indicatif::{ProgressBar, ProgressStyle};

pub struct DownloadProgress {
    bar: ProgressBar,
}

impl DownloadProgress {
    pub fn new(filename: &str, total_size: u64) -> anyhow::Result<Self> {
        let bar = ProgressBar::new(total_size);
        bar.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{msg}\n[{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})"
                )
                .map_err(|e| anyhow::anyhow!("进度条模板设置失败: {}", e))?
                .progress_chars("##-"),
        );
        bar.set_message(filename.to_string());
        Ok(Self { bar })
    }

    pub fn inc(&self, n: u64) {
        self.bar.inc(n);
    }

    pub fn set_position(&self, pos: u64) {
        self.bar.set_position(pos);
    }

    pub fn finish(&self) {
        self.bar
            .finish_with_message(format!("{} ✓", self.bar.message()));
    }
}

pub struct DownloadProgressUnknown {
    bar: ProgressBar,
}

impl DownloadProgressUnknown {
    pub fn new(filename: &str) -> anyhow::Result<Self> {
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg} ({bytes} downloaded)")
                .map_err(|e| anyhow::anyhow!("进度条模板设置失败: {}", e))?,
        );
        bar.set_message(filename.to_string());
        bar.enable_steady_tick(std::time::Duration::from_millis(100));
        Ok(Self { bar })
    }

    pub fn inc(&self, n: u64) {
        self.bar.inc(n);
    }

    pub fn finish(&self) {
        self.bar
            .finish_with_message(format!("{} ✓", self.bar.message()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_progress_new() {
        let p = DownloadProgress::new("test.mkv", 1000).unwrap();
        p.inc(100);
        p.set_position(200);
        p.finish();
    }

    #[test]
    fn test_download_progress_unknown_new() {
        let p = DownloadProgressUnknown::new("test.mkv").unwrap();
        p.inc(100);
        p.finish();
    }

    #[test]
    fn test_download_progress_zero_size() {
        let p = DownloadProgress::new("empty.mkv", 0).unwrap();
        p.finish();
    }
}
