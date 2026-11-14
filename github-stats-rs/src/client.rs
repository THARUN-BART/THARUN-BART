use reqwest::blocking::{Client, Response};
use serde_json::{json, Value};
use std::thread::sleep;
use std::time::Duration;

#[derive(Default, Debug, Clone)]
pub struct QueryCounts {
    pub user_getter: u32,
    pub follower_getter: u32,
    pub graph_repos_stars: u32,
    pub recursive_loc: u32,
    pub loc_query: u32,
}

impl QueryCounts {
    pub fn total(&self) -> u32 {
        self.user_getter + self.follower_getter + self.graph_repos_stars + self.recursive_loc + self.loc_query
    }
}

pub struct RepoMetadata {
    pub name_with_owner: String,
    pub total_commits: u64,
}

pub struct GithubClient {
    client: Client,
    token: String,
    pub username: String,
    pub counts: QueryCounts,
}

impl GithubClient {
    pub fn new(token: String, username: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            token,
            username,
            counts: QueryCounts::default(),
        }
    }

    /// Executes GraphQL query with exponential backoff and retry handling
    pub fn graphql_request(&self, query: &str, variables: Value, retries: usize) -> Result<Value, Box<dyn std::error::Error>> {
        let url = "https://api.github.com/graphql";
        let transient_statuses = [429, 500, 502, 503, 504];

        for attempt in 0..retries {
            let res: Result<Response, reqwest::Error> = self
                .client
                .post(url)
                .header("Authorization", format!("token {}", self.token))
                .header("User-Agent", &self.username)
                .json(&json!({
                    "query": query,
                    "variables": variables
                }))
                .send();

            match res {
                Ok(response) => {
                    let status = response.status().as_u16();

                    if !transient_statuses.contains(&status) {
                        let json_body: Value = response.json()?;
                        if let Some(errors) = json_body.get("errors") {
                            return Err(format!("GraphQL error: {:?}", errors).into());
                        }
                        return Ok(json_body);
                    }

                    if attempt == retries - 1 {
                        let text = response.text().unwrap_or_default();
                        return Err(format!("GitHub returned HTTP {}: {}", status, text).into());
                    }

                    let delay_secs = if let Some(retry_after) = response.headers().get("Retry-After") {
                        retry_after
                            .to_str()
                            .ok()
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(2u64.pow(attempt as u32).min(30))
                    } else {
                        2u64.pow(attempt as u32).min(30)
                    };

                    eprintln!("GitHub returned HTTP {}. Retrying in {}s...", status, delay_secs);
                    sleep(Duration::from_secs(delay_secs));
                }
                Err(err) => {
                    if attempt == retries - 1 {
                        return Err(Box::new(err));
                    }
                    let delay_secs = 2u64.pow(attempt as u32).min(30);
                    eprintln!("GitHub request error: {}. Retrying in {}s...", err, delay_secs);
                    sleep(Duration::from_secs(delay_secs));
                }
            }
        }

        Err("Unexpected retry loop exit".into())
    }

    /// Fetches user ID and account creation timestamp
    pub fn user_getter(&mut self, username: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
        self.counts.user_getter += 1;
        let query = r#"
            query($login: String!) {
                user(login: $login) {
                    id
                    createdAt
                }
            }
        "#;
        let variables = json!({ "login": username });
        let res = self.graphql_request(query, variables, 5)?;
        let user = &res["data"]["user"];
        let id = user["id"].as_str().ok_or("Missing user ID")?.to_string();
        let created_at = user["createdAt"].as_str().unwrap_or_default().to_string();
        Ok((id, created_at))
    }

    /// Fetches total followers for a user
    pub fn follower_getter(&mut self, username: &str) -> Result<u64, Box<dyn std::error::Error>> {
        self.counts.follower_getter += 1;
        let query = r#"
            query($login: String!) {
                user(login: $login) {
                    followers {
                        totalCount
                    }
                }
            }
        "#;
        let variables = json!({ "login": username });
        let res = self.graphql_request(query, variables, 5)?;
        let count = res["data"]["user"]["followers"]["totalCount"].as_u64().unwrap_or(0);
        Ok(count)
    }

    /// Queries repositories count or aggregates total stars across repositories
    pub fn graph_repos_stars(&mut self, count_type: &str, owner_affiliations: &[&str]) -> Result<u64, Box<dyn std::error::Error>> {
        let query = r#"
            query ($owner_affiliation: [RepositoryAffiliation], $login: String!, $cursor: String) {
                user(login: $login) {
                    repositories(first: 100, after: $cursor, ownerAffiliations: $owner_affiliation) {
                        totalCount
                        edges {
                            node {
                                ... on Repository {
                                    nameWithOwner
                                    stargazers {
                                        totalCount
                                    }
                                }
                            }
                        }
                        pageInfo {
                            endCursor
                            hasNextPage
                        }
                    }
                }
            }
        "#;

        let mut cursor: Option<String> = None;
        let mut total_stars = 0;

        loop {
            self.counts.graph_repos_stars += 1;
            let variables = json!({
                "owner_affiliation": owner_affiliations,
                "login": self.username,
                "cursor": cursor
            });

            let res = self.graphql_request(query, variables, 5)?;
            let repos = &res["data"]["user"]["repositories"];

            if count_type == "repos" {
                return Ok(repos["totalCount"].as_u64().unwrap_or(0));
            }

            if let Some(edges) = repos["edges"].as_array() {
                for edge in edges {
                    if let Some(stars) = edge["node"]["stargazers"]["totalCount"].as_u64() {
                        total_stars += stars;
                    }
                }
            }

            let has_next_page = repos["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false);
            if !has_next_page {
                break;
            }
            cursor = repos["pageInfo"]["endCursor"].as_str().map(|s| s.to_string());
        }

        Ok(total_stars)
    }

    /// Lists repositories for LOC calculation (with default branch commit count)
    pub fn loc_query(&mut self, owner_affiliations: &[&str]) -> Result<Vec<RepoMetadata>, Box<dyn std::error::Error>> {
        let query = r#"
            query ($owner_affiliation: [RepositoryAffiliation], $login: String!, $cursor: String) {
                user(login: $login) {
                    repositories(first: 60, after: $cursor, ownerAffiliations: $owner_affiliation) {
                        edges {
                            node {
                                ... on Repository {
                                    nameWithOwner
                                    defaultBranchRef {
                                        target {
                                            ... on Commit {
                                                history {
                                                    totalCount
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        pageInfo {
                            endCursor
                            hasNextPage
                        }
                    }
                }
            }
        "#;

        let mut cursor: Option<String> = None;
        let mut list = Vec::new();

        loop {
            self.counts.loc_query += 1;
            let variables = json!({
                "owner_affiliation": owner_affiliations,
                "login": self.username,
                "cursor": cursor
            });

            let res = self.graphql_request(query, variables, 5)?;
            let repos = &res["data"]["user"]["repositories"];

            if let Some(edges) = repos["edges"].as_array() {
                for edge in edges {
                    let node = &edge["node"];
                    if let Some(name) = node["nameWithOwner"].as_str() {
                        let total_commits = node["defaultBranchRef"]["target"]["history"]["totalCount"]
                            .as_u64()
                            .unwrap_or(0);
                        list.push(RepoMetadata {
                            name_with_owner: name.to_string(),
                            total_commits,
                        });
                    }
                }
            }

            let has_next_page = repos["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false);
            if !has_next_page {
                break;
            }
            cursor = repos["pageInfo"]["endCursor"].as_str().map(|s| s.to_string());
        }

        Ok(list)
    }

    /// Paginates through repository commits authored by author_id, summing additions and deletions
    pub fn recursive_loc(
        &mut self,
        owner: &str,
        repo_name: &str,
        author_id: &str,
    ) -> Result<(u64, u64, u64), Box<dyn std::error::Error>> {
        let query = r#"
            query ($repo_name: String!, $owner: String!, $cursor: String, $author_id: ID!) {
                repository(name: $repo_name, owner: $owner) {
                    defaultBranchRef {
                        target {
                            ... on Commit {
                                history(first: 100, after: $cursor, author: { id: $author_id }) {
                                    totalCount
                                    edges {
                                        node {
                                            ... on Commit {
                                                committedDate
                                            }
                                            author {
                                                user {
                                                    id
                                                }
                                            }
                                            deletions
                                            additions
                                        }
                                    }
                                    pageInfo {
                                        endCursor
                                        hasNextPage
                                    }
                                }
                            }
                        }
                    }
                }
            }
        "#;

        let mut cursor: Option<String> = None;
        let mut additions_total: u64 = 0;
        let mut deletions_total: u64 = 0;
        let mut my_commits: u64 = 0;

        loop {
            self.counts.recursive_loc += 1;
            let variables = json!({
                "repo_name": repo_name,
                "owner": owner,
                "cursor": cursor,
                "author_id": author_id
            });

            let res = self.graphql_request(query, variables, 5)?;
            let repo = &res["data"]["repository"];

            if repo.is_null() {
                break;
            }

            let default_branch = &repo["defaultBranchRef"];
            if default_branch.is_null() {
                break;
            }

            let history = &default_branch["target"]["history"];
            if let Some(edges) = history["edges"].as_array() {
                for edge in edges {
                    let node = &edge["node"];
                    let user_id = node["author"]["user"]["id"].as_str();
                    if user_id == Some(author_id) {
                        my_commits += 1;
                        additions_total += node["additions"].as_u64().unwrap_or(0);
                        deletions_total += node["deletions"].as_u64().unwrap_or(0);
                    }
                }
            }

            let has_next_page = history["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false);
            if !has_next_page {
                break;
            }
            cursor = history["pageInfo"]["endCursor"].as_str().map(|s| s.to_string());
        }

        Ok((additions_total, deletions_total, my_commits))
    }
}
