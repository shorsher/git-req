use crate::git;
use log::{debug, error, info, trace};
use regex::Regex;
use reqwest;
use serde_derive::{Deserialize, Serialize};
use std::fmt;
use std::io::{stdin, stdout, Write};

pub trait Remote {
    /// Get the ID of the project associated with the repository
    fn get_project_id(&mut self) -> Result<&str, &str>;

    /// Get the branch associated with the merge request having the given ID
    fn get_req_branch(&mut self, mr_id: i64) -> Result<String, &str>;

    /// Get the names of the merge/pull requests opened against the remote
    fn get_req_names(&mut self) -> Result<Vec<MergeRequest>, &str>;
}

/// Print a pretty remote
impl fmt::Display for Remote {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Remote")
    }
}

/// Debug a remote
impl fmt::Debug for Remote {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MergeRequest {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub source_branch: String,
}

#[derive(Debug)]
struct GitHub {
    id: String,
    name: String,
    origin: String,
    api_root: String,
    api_key: String,
}

impl Remote for GitHub {
    fn get_project_id(&mut self) -> Result<&str, &str> {
        Ok(&self.id)
    }

    fn get_req_branch(&mut self, mr_id: i64) -> Result<String, &str> {
        Ok(format!("pr/{}", mr_id))
    }

    fn get_req_names(&mut self) -> Result<Vec<MergeRequest>, &str> {
        retrieve_github_project_pull_requests(self)
    }
}

#[derive(Debug)]
struct GitLab {
    id: String,
    domain: String,
    name: String,
    namespace: String,
    origin: String,
    api_root: String,
    api_key: String,
}

impl Remote for GitLab {
    fn get_project_id(&mut self) -> Result<&str, &str> {
        if self.id.is_empty() {
            self.id = format!("{}", query_gitlab_project_id(self)?);
        }
        Ok(&self.id)
    }

    fn get_req_branch(&mut self, mr_id: i64) -> Result<String, &str> {
        query_gitlab_branch_name(self, mr_id)
    }

    fn get_req_names(&mut self) -> Result<Vec<MergeRequest>, &str> {
        retrieve_gitlab_project_merge_requests(self)
    }
}

fn query_github_api(url: reqwest::Url, token: String) -> reqwest::Response {
    let client = reqwest::Client::new();
    client
        .get(url)
        .header("Authorization", format!("token {}", token))
        .send()
        .expect("failed to send request")
}

#[derive(Serialize, Deserialize, Debug)]
struct GitLabProject {
    id: i64,
    description: Option<String>,
    name: String,
    path: String,
    path_with_namespace: String,
}

fn query_gitlab_api(url: reqwest::Url, token: String) -> reqwest::Response {
    let client = reqwest::Client::new();
    client
        .get(url)
        .header("PRIVATE-TOKEN", token)
        .send()
        .expect("failed to send request")
}

/// Query the GitLab API for remote's project
fn query_gitlab_project_id(remote: &GitLab) -> Result<i64, &'static str> {
    trace!("Querying GitLab Project API for {:?}", remote);
    let url = reqwest::Url::parse(&format!(
        "{}/projects/{}%2F{}",
        remote.api_root, remote.namespace, remote.name
    ))
    .unwrap();
    let mut resp = query_gitlab_api(url, remote.api_key.to_string());
    debug!("Project ID query response: {:?}", resp);
    if !resp.status().is_success() {
        match search_gitlab_project_id(remote) {
            Ok(id) => {
                return Ok(id);
            }
            Err(_) => {
                return Err(
                    "Unable to get the project ID from the GitLab API.\nFind and configure \
                     your project ID using the instructions at: \
                     https://github.com/arusahni/git-req/wiki/Finding-Project-IDs",
                );
            }
        }
    }
    let buf: GitLabProject = resp.json().expect("failed to read response");
    debug!("{:?}", buf);
    Ok(buf.id)
}

fn gitlab_to_mr(req: GitLabMergeRequest) -> MergeRequest {
    MergeRequest {
        id: req.iid,
        title: req.title,
        description: req.description,
        source_branch: req.source_branch,
    }
}

fn github_to_mr(req: GitHubPullRequest) -> MergeRequest {
    MergeRequest {
        id: req.number,
        title: req.title,
        description: req.body,
        source_branch: format!("pr/{}", req.number),
    }
}

fn retrieve_github_project_pull_requests(
    remote: &GitHub,
) -> Result<Vec<MergeRequest>, &'static str> {
    trace!("Querying for GitHub PR for {:?}", remote);
    let url = reqwest::Url::parse(&format!("{}/{}/pulls", remote.api_root, remote.id)).unwrap();
    let mut resp = query_github_api(url, remote.api_key.to_string());
    debug!("PR list query response: {:?}", resp);
    let buf: Vec<GitHubPullRequest> = match resp.json() {
        Ok(buf) => buf,
        Err(_) => {
            return Err("failed to read API response");
        }
    };
    Ok(buf.into_iter().map(github_to_mr).collect())
}

fn retrieve_gitlab_project_merge_requests(
    remote: &GitLab,
) -> Result<Vec<MergeRequest>, &'static str> {
    trace!("Querying GitLab MR for {:?}", remote);
    let url = reqwest::Url::parse(&format!(
        "{}/projects/{}/merge_requests?state=opened",
        remote.api_root, remote.id
    ))
    .unwrap();
    let mut resp = query_gitlab_api(url, remote.api_key.to_string());
    debug!("MR list query response: {:?}", resp);
    let buf: Vec<GitLabMergeRequest> = match resp.json() {
        Ok(buf) => buf,
        Err(_) => {
            return Err("failed to read response");
        }
    };
    Ok(buf.into_iter().map(gitlab_to_mr).collect())
}

#[derive(Serialize, Deserialize, Debug)]
struct GitLabNamespace {
    id: i64,
    name: String,
    path: String,
    kind: String,
    full_path: String,
}

/// Search GitLab for the project ID (if the direct lookup didn't work)
fn search_gitlab_project_id(remote: &GitLab) -> Result<i64, &'static str> {
    trace!(
        "Searching GitLab API for namespace {:?} by project name",
        remote.namespace
    );
    let url = reqwest::Url::parse(&format!(
        "{}/namespaces/{}",
        remote.api_root, remote.namespace
    ))
    .unwrap();
    let mut resp = query_gitlab_api(url, remote.api_key.to_string());
    debug!("Namespace ID query response: {:?}", resp);
    if !resp.status().is_success() {
        return Err("Couldn't find namespace");
    }
    let ns_buf: GitLabNamespace = resp.json().expect("failed to read response");
    debug!("Querying namespace {:?}", ns_buf);
    let url = match ns_buf.kind.as_ref() {
        "user" => reqwest::Url::parse(&format!("{}/users/{}/projects", remote.api_root, ns_buf.id))
            .unwrap(),
        "group" => reqwest::Url::parse(&format!(
            "{}/groups/{}/projects?search={}",
            remote.api_root, ns_buf.id, remote.name
        ))
        .unwrap(),
        _ => {
            error!("Unknown namespace kind {:?}", ns_buf.kind);
            return Err("Unknown namespace");
        }
    };
    let mut resp = query_gitlab_api(url, remote.api_key.to_string());
    debug!("Project ID query response: {:?}", resp);
    let projects: Vec<GitLabProject> = resp.json().expect("failed to read projects response");
    match projects.iter().find(|&prj| prj.name == remote.name) {
        Some(project) => Ok(project.id),
        None => Err("Couldn't find project"),
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct GitHubPullRequest {
    id: i64,
    number: i64,
    title: String,
    body: Option<String>,
    html_url: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct GitLabMergeRequest {
    id: i64,
    iid: i64,
    title: String,
    description: Option<String>,
    target_branch: String,
    source_branch: String,
    sha: String,
    web_url: String,
}

/// Get the project ID from config
fn load_project_id() -> Option<String> {
    match git::get_config("projectid") {
        Some(project_id) => Some(project_id),
        None => {
            debug!("No project ID found");
            None
        }
    }
}

/// Query the GitLab API for the branch corresponding to the MR
fn query_gitlab_branch_name(remote: &GitLab, mr_id: i64) -> Result<String, &str> {
    let client = reqwest::Client::new();
    let url = reqwest::Url::parse(&format!(
        "{}/projects/{}/merge_requests/{}",
        remote.api_root, remote.id, mr_id
    ))
    .unwrap();
    let mut resp = client
        .get(url)
        .header("PRIVATE-TOKEN", remote.api_key.to_string())
        .send()
        .expect("failed to send request");
    debug!("Response: {:?}", resp);
    let buf: GitLabMergeRequest = match resp.json() {
        Ok(buf) => buf,
        Err(_) => {
            return Err("failed to read response");
        }
    };
    Ok(buf.source_branch)
}

/// Extract the project name from a Github origin URL
fn get_github_project_name(origin: &str) -> String {
    trace!("Getting project name for: {}", origin);
    let project_regex = Regex::new(r".*:(.*/\S+)\.git\w*$").unwrap();
    let captures = project_regex.captures(origin).unwrap();
    String::from(&captures[1])
}

/// Extract the project name from a GitLab origin URL
fn get_gitlab_project_name(origin: &str) -> String {
    trace!("Getting project name for: {}", origin);
    let project_regex = Regex::new(r".*/(\S+)\.git$").unwrap();
    let captures = project_regex.captures(origin).unwrap();
    String::from(&captures[1])
}

/// Extract the project namespace from a GitLab origin URL
fn get_gitlab_project_namespace(origin: &str) -> Option<String> {
    trace!("Getting project namespace for: {}", origin);
    let project_regex = Regex::new(r".*[/:](\S+)/\S+\.git$").unwrap();
    match project_regex.captures(origin) {
        Some(captures) => Some(String::from(&captures[1])),
        None => None,
    }
}

/// Get the domain from an origin URL
pub fn get_domain(origin: &str) -> Result<&str, String> {
    let domain_regex = Regex::new(r"((http[s]?|ssh)://)?(\S+@)?(?P<domain>([^:/])+)").unwrap();
    let captures = domain_regex.captures(origin);
    if captures.is_none() {
        return Err(String::from("invalid remote set"));
    }
    Ok(captures.unwrap().name("domain").map_or("", |x| x.as_str()))
}

fn get_api_key(domain: &str) -> String {
    match git::get_req_config(&domain, "apikey") {
        Some(key) => key,
        None => {
            let mut newkey = String::new();
            println!("No API token for {} found. See https://github.com/arusahni/git-req/wiki/API-Keys for instructions.", domain);
            print!("{} API token: ", domain);
            let _ = stdout().flush();
            stdin()
                .read_line(&mut newkey)
                .expect("Did not input a correct key");
            trace!("New Key: {}", &newkey);
            git::set_req_config(&domain, "apikey", &newkey.trim());
            String::from(newkey.trim())
        }
    }
}

/// Get a remote struct from an origin URL
pub fn get_remote(origin: &str) -> Result<Box<Remote>, String> {
    let domain = get_domain(origin)?;
    Ok(match domain {
        "github.com" => {
            let mut remote = GitHub {
                id: get_github_project_name(origin),
                name: get_github_project_name(origin),
                origin: String::from(origin),
                api_root: String::from("https://api.github.com/repos"),
                api_key: String::from(""),
            };
            let apikey = get_api_key("github.com");
            info!("API Key: {}", &apikey);
            remote.api_key = apikey;
            Box::new(remote)
        }
        // For now, if not GitHub, then GitLab
        gitlab_domain => {
            let namespace = match get_gitlab_project_namespace(origin) {
                Some(ns) => ns,
                None => {
                    return Err(String::from(
                        "Could not parse the GitLab project namespace from the origin.",
                    ));
                }
            };
            let mut remote = GitLab {
                id: String::from(""),
                domain: String::from(gitlab_domain),
                name: get_gitlab_project_name(origin),
                namespace,
                origin: String::from(origin),
                api_root: format!("https://{}/api/v4", gitlab_domain),
                api_key: String::from(""),
            };
            let apikey = get_api_key(&domain);
            info!("API Key: {}", &apikey);
            remote.api_key = apikey;
            let project_id = match load_project_id() {
                Some(x) => x,
                None => {
                    let project_id_str = match remote.get_project_id() {
                        Ok(id_str) => Ok(id_str),
                        Err(e) => {
                            info!("Error getting project ID: {:?}", e);
                            Err(e)
                        }
                    }?;
                    git::set_config("projectid", project_id_str);
                    String::from(project_id_str)
                }
            };
            info!("Got project ID: {}", project_id);
            remote.id = project_id;
            Box::new(remote)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_gitlab_project_namespace_http() {
        let ns = get_gitlab_project_namespace("https://gitlab.com/my_namespace/my_project.git");
        assert!(ns.is_some());
        assert_eq!("my_namespace", ns.unwrap());
    }

    #[test]
    fn test_get_gitlab_project_namespace_git() {
        let ns = get_gitlab_project_namespace("git@gitlab.com:my_namespace/my_project.git");
        assert!(ns.is_some());
        assert_eq!("my_namespace", ns.unwrap());
    }

    #[test]
    fn test_get_gitlab_project_name_http() {
        let ns = get_gitlab_project_name("https://gitlab.com/my_namespace/my_project.git");
        assert_eq!("my_project", ns);
    }

    #[test]
    fn test_get_gitlab_project_name_git() {
        let ns = get_gitlab_project_name("git@gitlab.com:my_namespace/my_project.git");
        assert_eq!("my_project", ns);
    }
}
