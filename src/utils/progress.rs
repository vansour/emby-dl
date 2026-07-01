use indicatif::{ProgressBar, ProgressStyle};

const MAX_MSG_LEN: usize = 45;

fn truncate(s: &str) -> String {
    if s.len() > MAX_MSG_LEN {
        let r: String = s.chars().take(MAX_MSG_LEN - 2).collect();
        format!("{}..", r)
    } else {
        s.to_string()
    }
}

pub struct DownloadProgress {
    bar: ProgressBar,
}

impl DownloadProgress {
    pub fn new(filename: &str, total_size: u64) -> anyhow::Result<Self> {
        let bar = ProgressBar::new(total_size);
        bar.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{msg}\n[{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}, {eta})"
                )
                .map_err(|e| anyhow::anyhow!("进度条模板设置失败: {}", e))?
                .progress_chars("##-"),
        );
        bar.set_message(truncate(filename));
        Ok(Self { bar })
    }

    pub fn inc(&self, n: u64) {
        self.bar.inc(n);
    }

    pub fn finish(&self) {
        self.bar
            .finish_with_message(format!("{} ✓", self.bar.message()));
    }

    pub fn abort(&self) {
        self.bar
            .finish_with_message(format!("{} ✗", self.bar.message()));
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
                .template("{spinner:.green} {msg} ({bytes}, {binary_bytes_per_sec})")
                .map_err(|e| anyhow::anyhow!("进度条模板设置失败: {}", e))?,
        );
        bar.set_message(truncate(filename));
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

    pub fn abort(&self) {
        self.bar
            .finish_with_message(format!("{} ✗", self.bar.message()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_progress_new() {
        let p = DownloadProgress::new("test.mkv", 1000).unwrap();
        p.inc(100);
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

    #[test]
    fn test_download_progress_abort() {
        let p = DownloadProgress::new("fail.mkv", 500).unwrap();
        p.inc(100);
        p.abort();
    }

    #[test]
    fn test_download_progress_unknown_abort() {
        let p = DownloadProgressUnknown::new("fail.mkv").unwrap();
        p.inc(50);
        p.abort();
    }

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("short.txt"), "short.txt");
    }

    #[test]
    fn test_truncate_long() {
        let long = "a".repeat(100);
        let result = truncate(&long);
        assert_eq!(result.len(), MAX_MSG_LEN);
        assert!(result.ends_with(".."));
    }

    #[test]
    fn test_truncate_exact_boundary() {
        let s = "a".repeat(MAX_MSG_LEN);
        assert_eq!(truncate(&s).len(), MAX_MSG_LEN);
    }

    #[test]
    fn test_truncate_chinese() {
        let s = "中".repeat(30);
        let result = truncate(&s);
        assert!(result.chars().count() <= MAX_MSG_LEN);
        assert!(result.ends_with(".."));
    }
}
