use crate::remotes::{MergeRequest, Remote};
use log::{debug, trace};
use regex::Regex;
use reqwest;
use serde_derive::{Deserialize, Serialize};

#[derive(Debug)]
pub struct Bitbucket {
    pub id: String,
    pub domain: String,
    pub name: String,
    pub origin: String,
    pub api_root: String,
    pub api_key: String,
}

// #[derive(Debug)]
// impl Remote for Bitbucket {
// }

fn query_bitbucket_api(url: reqwest::Url, token: String) -> reqwest::Response {
    let client = reqwest::Client::new();
    client
        .get(url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .expect("failed to send request")
}

pub fn get_bitbucket_project_name(origin: &str) -> String {
    trace!("Getting project name for: {}", origin);
    let project_regex = Regex::new(r".*:(.*/\S+)\.git\w*$").unwrap();
    let captures = project_regex.captures(origin).unwrap();
    String::from(&captures[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_bitbucket_project_name() {
        let name = get_bitbucket_project_name("git@bitbucket.org:shorsher/test.git");
        assert_eq!("shorsher/test", name);
    }
}