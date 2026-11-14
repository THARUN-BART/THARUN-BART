use github_stats_rs::cache::{CacheFile, RepoCacheEntry};

#[test]
fn test_sha256_hex_matches_python() {
    let hash = CacheFile::sha256_hex("THARUN-BART");
    assert_eq!(hash, "7da058eb6c351b99f79124dd8fee996e9bb04ab2a91d89a4cce47da464565db8");
}

#[test]
fn test_entry_parse_and_serialize() {
    let line = "e613a7d73bcd3e76b79587eafd1b07203f3a705bacf65f5309cec659b5096c33 5 5 821 49";
    let entry = RepoCacheEntry::parse_line(line).expect("failed to parse cache line");
    assert_eq!(entry.repo_hash, "e613a7d73bcd3e76b79587eafd1b07203f3a705bacf65f5309cec659b5096c33");
    assert_eq!(entry.total_commits, 5);
    assert_eq!(entry.my_commits, 5);
    assert_eq!(entry.loc_add, 821);
    assert_eq!(entry.loc_del, 49);
    assert_eq!(entry.to_line(), format!("{}\n", line));
}
