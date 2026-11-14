use chrono::NaiveDate;
use github_stats_rs::cache::CacheFile;
use github_stats_rs::client::GithubClient;
use github_stats_rs::config;
use github_stats_rs::date;
use github_stats_rs::svg::{self, SvgStats};
use std::path::Path;
use std::time::{Duration, Instant};

fn format_duration(label: &str, d: Duration) {
    let secs = d.as_secs_f64();
    if secs >= 1.0 {
        println!("   {:<20} {:>12.4} s ", format!("{}:", label), secs);
    } else {
        println!("   {:<20} {:>12.4} ms", format!("{}:", label), secs * 1000.0);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Calculation times:");

    // 1. Config
    let cfg = config::load();
    let mut gh = GithubClient::new(cfg.token, cfg.username.clone());

    // 2. Account data
    let t0 = Instant::now();
    let user_res = gh.user_getter(&cfg.username);
    let user_time = t0.elapsed();
    format_duration("account data", user_time);

    let (owner_id, _created_at) = match user_res {
        Ok((id, date)) => (id, date),
        Err(e) => {
            eprintln!("   [Note] Failed to query user from GitHub: {}", e);
            // Default placeholder ID
            ("MDQ6VXNlcjU3MzMxMTM0".to_string(), String::new())
        }
    };

    // 3. Age calculation
    let t0 = Instant::now();
    let birthday = NaiveDate::from_ymd_opt(2006, 11, 29).unwrap();
    let age_data = date::daily_readme(birthday);
    let age_time = t0.elapsed();
    format_duration("age calculation", age_time);

    // 4. LOC & Cache
    let cache_path = CacheFile::get_cache_path(&cfg.username);
    let mut cache = CacheFile::load(&cache_path, 7)?;

    let t0 = Instant::now();
    let affiliations = ["OWNER", "COLLABORATOR", "ORGANIZATION_MEMBER"];
    let loc_repos_res = gh.loc_query(&affiliations);
    let loc_time = t0.elapsed();

    let mut is_cached = true;
    if let Ok(repos) = loc_repos_res {
        let repo_hashes: Vec<String> = repos.iter().map(|r| CacheFile::sha256_hex(&r.name_with_owner)).collect();
        if cache.entries.len() != repos.len() {
            is_cached = false;
            let _ = cache.flush_initial(&repo_hashes);
        }

        for (i, repo) in repos.iter().enumerate() {
            let hash = &repo_hashes[i];
            if i < cache.entries.len() && cache.entries[i].repo_hash == *hash {
                if cache.entries[i].total_commits != repo.total_commits {
                    is_cached = false;
                    println!("      [{}/{}] Updating {}...", i + 1, repos.len(), repo.name_with_owner);
                    if let Some((owner, repo_name)) = repo.name_with_owner.split_once('/') {
                        if let Ok((add, del, my_c)) = gh.recursive_loc(owner, repo_name, &owner_id) {
                            cache.entries[i].total_commits = repo.total_commits;
                            cache.entries[i].my_commits = my_c;
                            cache.entries[i].loc_add = add;
                            cache.entries[i].loc_del = del;
                            let _ = cache.save();
                        }
                    }
                }
            }
        }
    }

    if is_cached {
        format_duration("LOC (cached)", loc_time);
    } else {
        format_duration("LOC (no cache)", loc_time);
    }

    // 5. Commit data
    let t0 = Instant::now();
    let commit_data = cache.total_my_commits();
    let commit_time = t0.elapsed();

    // 6. Stars
    let t0 = Instant::now();
    let star_data = gh.graph_repos_stars("stars", &["OWNER"]).unwrap_or(0);
    let star_time = t0.elapsed();

    // 7. Repos
    let t0 = Instant::now();
    let repo_data = gh.graph_repos_stars("repos", &["OWNER"]).unwrap_or(0);
    let repo_time = t0.elapsed();

    // 8. Contributed repos
    let t0 = Instant::now();
    let contrib_data = gh.graph_repos_stars("repos", &["OWNER", "COLLABORATOR", "ORGANIZATION_MEMBER"]).unwrap_or(cache.entries.len() as u64);
    let contrib_time = t0.elapsed();

    // 9. Followers
    let t0 = Instant::now();
    let follower_data = gh.follower_getter(&cfg.username).unwrap_or(0);
    let follower_time = t0.elapsed();

    let (loc_add, loc_del, loc_net) = cache.total_loc();

    // 10. SVG Update (checks current directory and parent directory)
    let svg_path = if Path::new("dark_mode.svg").exists() {
        Some(std::path::PathBuf::from("dark_mode.svg"))
    } else if Path::new("../dark_mode.svg").exists() {
        Some(std::path::PathBuf::from("../dark_mode.svg"))
    } else {
        None
    };

    if let Some(ref path) = svg_path {
        let stats = SvgStats {
            age: age_data,
            commits: commit_data,
            stars: star_data,
            repos: repo_data,
            contribs: contrib_data,
            followers: follower_data,
            loc_net,
            loc_add,
            loc_del,
        };

        if let Err(e) = svg::update_svg(path, &stats) {
            eprintln!("Failed to update SVG: {}", e);
        } else {
            println!("Updated {}", path.display());
        }
    } else {
        eprintln!("Warning: dark_mode.svg not found in current or parent directory!");
    }

    let total_time = user_time + age_time + loc_time + commit_time + star_time + repo_time + contrib_time + follower_time;
    println!("\nTotal function time: {:.4} s", total_time.as_secs_f64());
    println!("Total GitHub GraphQL API calls: {:>3}", gh.counts.total());
    println!("   user_getter:          {}", gh.counts.user_getter);
    println!("   follower_getter:      {}", gh.counts.follower_getter);
    println!("   graph_repos_stars:    {}", gh.counts.graph_repos_stars);
    println!("   recursive_loc:        {}", gh.counts.recursive_loc);
    println!("   loc_query:            {}", gh.counts.loc_query);

    Ok(())
}
