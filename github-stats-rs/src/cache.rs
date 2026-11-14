use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoCacheEntry {
    pub repo_hash: String,
    pub total_commits: u64,
    pub my_commits: u64,
    pub loc_add: u64,
    pub loc_del: u64,
}

impl RepoCacheEntry {
    pub fn to_line(&self) -> String {
        format!(
            "{} {} {} {} {}\n",
            self.repo_hash, self.total_commits, self.my_commits, self.loc_add, self.loc_del
        )
    }

    pub fn parse_line(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            return None;
        }

        Some(Self {
            repo_hash: parts[0].to_string(),
            total_commits: parts[1].parse().unwrap_or(0),
            my_commits: parts[2].parse().unwrap_or(0),
            loc_add: parts[3].parse().unwrap_or(0),
            loc_del: parts[4].parse().unwrap_or(0),
        })
    }
}

pub struct CacheFile {
    pub path: PathBuf,
    pub comments: Vec<String>,
    pub entries: Vec<RepoCacheEntry>,
}

impl CacheFile {
    /// Computes the SHA-256 hash as a hex string
    pub fn sha256_hex(input: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Resolves the cache file path for a given username (checks both current and parent directory)
    pub fn get_cache_path(username: &str) -> PathBuf {
        let filename = format!("{}.txt", Self::sha256_hex(username));
        if Path::new("cache").exists() {
            Path::new("cache").join(filename)
        } else if Path::new("../cache").exists() {
            Path::new("../cache").join(filename)
        } else {
            Path::new("cache").join(filename)
        }
    }

    /// Loads cache entries from file or initializes if not present
    pub fn load(path: &Path, comment_size: usize) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut comments = Vec::new();
        let mut entries = Vec::new();

        if path.exists() {
            let file = File::open(path)?;
            let reader = BufReader::new(file);

            for (idx, line_res) in reader.lines().enumerate() {
                let line = line_res?;
                if idx < comment_size {
                    comments.push(line);
                } else if let Some(entry) = RepoCacheEntry::parse_line(&line) {
                    entries.push(entry);
                }
            }
        } else {
            // Fill default comments if file is new
            for _ in 0..comment_size {
                comments.push("This line is a comment block. Write whatever you want here.".to_string());
            }
        }

        Ok(Self {
            path: path.to_path_buf(),
            comments,
            entries,
        })
    }

    /// Atomically writes cache comments and entries to disk
    pub fn save(&self) -> io::Result<()> {
        let tmp_path = self.path.with_extension("tmp");

        {
            let mut file = File::create(&tmp_path)?;
            for comment in &self.comments {
                writeln!(file, "{}", comment)?;
            }
            for entry in &self.entries {
                file.write_all(entry.to_line().as_bytes())?;
            }
            file.flush()?;
        }

        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }

    /// Flushes cache with initial entries initialized to 0
    pub fn flush_initial(&mut self, repo_hashes: &[String]) -> io::Result<()> {
        self.entries = repo_hashes
            .iter()
            .map(|hash| RepoCacheEntry {
                repo_hash: hash.clone(),
                total_commits: 0,
                my_commits: 0,
                loc_add: 0,
                loc_del: 0,
            })
            .collect();
        self.save()
    }

    /// Calculates total additions, deletions, and net LOC
    pub fn total_loc(&self) -> (u64, u64, i64) {
        let mut add = 0;
        let mut del = 0;

        for entry in &self.entries {
            add += entry.loc_add;
            del += entry.loc_del;
        }

        (add, del, add as i64 - del as i64)
    }

    /// Sums my_commits across all cached repositories
    pub fn total_my_commits(&self) -> u64 {
        self.entries.iter().map(|e| e.my_commits).sum()
    }
}
