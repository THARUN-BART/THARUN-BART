use std::fs;
use std::path::Path;

pub struct SvgStats {
    pub age: String,
    pub commits: u64,
    pub stars: u64,
    pub repos: u64,
    pub contribs: u64,
    pub followers: u64,
    pub loc_net: i64,
    pub loc_add: u64,
    pub loc_del: u64,
}

pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let len = s.len();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result
}

pub fn format_number_i64(n: i64) -> String {
    if n < 0 {
        format!("-{}", format_number((-n) as u64))
    } else {
        format_number(n as u64)
    }
}

pub fn calculate_dots(text: &str, length: usize) -> String {
    let text_len = text.chars().count();
    let just_len = if length > text_len { length - text_len } else { 0 };

    match just_len {
        0 => String::new(),
        1 => " ".to_string(),
        2 => ". ".to_string(),
        n => format!(" {} ", ".".repeat(n)),
    }
}

/// Replaces the inner text of a tag with a specific id="..." while preserving the rest of the file
pub fn replace_tag_content(svg: &str, element_id: &str, new_content: &str) -> String {
    let id_needle = format!("id=\"{}\"", element_id);

    if let Some(id_pos) = svg.find(&id_needle) {
        // Find closing bracket '>' of the opening tag
        if let Some(tag_end) = svg[id_pos..].find('>') {
            let content_start = id_pos + tag_end + 1;
            // Find opening bracket '<' of the closing tag
            if let Some(content_end_rel) = svg[content_start..].find('<') {
                let content_end = content_start + content_end_rel;
                let mut result = String::with_capacity(svg.len() + new_content.len());
                result.push_str(&svg[..content_start]);
                result.push_str(new_content);
                result.push_str(&svg[content_end..]);
                return result;
            }
        }
    }

    svg.to_string()
}

pub fn justify_format(svg: &mut String, element_id: &str, new_text: &str, length: usize) {
    *svg = replace_tag_content(svg, element_id, new_text);
    let dots = calculate_dots(new_text, length);
    *svg = replace_tag_content(svg, &format!("{}_dots", element_id), &dots);
}

pub fn update_svg<P: AsRef<Path>>(svg_path: P, stats: &SvgStats) -> Result<(), Box<dyn std::error::Error>> {
    let path = svg_path.as_ref();
    let content = fs::read_to_string(path)?;
    let mut updated = content;

    justify_format(&mut updated, "age_data", &stats.age, 0);
    justify_format(&mut updated, "commit_data", &format_number(stats.commits), 22);
    justify_format(&mut updated, "star_data", &format_number(stats.stars), 14);
    justify_format(&mut updated, "repo_data", &format_number(stats.repos), 6);
    justify_format(&mut updated, "contrib_data", &format_number(stats.contribs), 0);
    justify_format(&mut updated, "follower_data", &format_number(stats.followers), 10);
    justify_format(&mut updated, "loc_data", &format_number_i64(stats.loc_net), 9);
    justify_format(&mut updated, "loc_add", &format_number(stats.loc_add), 0);
    justify_format(&mut updated, "loc_del", &format_number(stats.loc_del), 7);

    fs::write(path, updated)?;

    Ok(())
}
