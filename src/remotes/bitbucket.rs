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

#[derive(Serialize, Deserialize, Debug)]
struct BitbucketPullRequest {
    id: i64,
    title: String,
    summary: Option<String>,
    html_url: String,
}

impl Remote for Bitbucket {
    fn get_domain(&mut self) -> &str {
        &self.domain
    }

    fn get_project_id(&mut self) -> Result<&str, &str> {
        Ok(&self.id)
    }

    fn has_useful_branch_names(&mut self) -> bool {
        false
    }

    fn get_local_req_branch(&mut self, mr_id: i64) -> Result<String, &str> {
        Ok(format!("pr/{mr_id}", mr_id = mr_id))
    }

    fn get_remote_req_branch(&mut self, mr_id: i64) -> Result<String, &str> {
        Ok(format!("pull/{mr_id}/head", mr_id = mr_id))
    }

    fn get_req_names(&mut self) -> Result<Vec<MergeRequest>, &str> {
        retrieve_bitbucket_project_pull_requests(self)
    }
}

fn query_bitbucket_api(url: reqwest::Url, token: String) -> reqwest::Response {
    let client = reqwest::Client::new();
    client
        .get(url)
        .basic_auth("shorsher", Some("RhXcSmcPDdJaAQRDVCCb"))
        .send()
        .expect("failed to send request")
}

fn bitbucket_to_mr(req: BitbucketPullRequest) -> MergeRequest {
    MergeRequest {
        id: req.id,
        title: req.title,
        description: req.summary,
        source_branch: format!("pullrequests/{}", req.id),
    }
}

fn retrieve_bitbucket_project_pull_requests(
    remote: &Bitbucket
) -> Result<Vec<MergeRequest>, &'static str> {
    trace!("Querying for Bitbucket PR for {:?}", remote);
    let url = reqwest::Url::parse(&format!("{}/{}/pullrequests", remote.api_root, remote.id)).unwrap();
    let mut resp = query_bitbucket_api(url, remote.api_root.to_string());
    debug!("PR list query response: {:?}", resp);
    let buf: Vec<BitbucketPullRequest> = match resp.json() {
        Ok(buf) => buf,
        Err(_) => {
            return Err("failed to read API response");
        }
    };
    Ok(buf.into_iter().map(bitbucket_to_mr).collect())
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