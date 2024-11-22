pub mod github {

    use std::sync::Arc;

    use regex::Regex;
    use tokio::{
        sync::{RwLock, RwLockReadGuard},
        task::JoinSet,
    };

    use crate::parsing::parsing::JSONRepo;

    use super::super::parsing::parsing::Config;
    use octocrab::{repos::RepoHandler, Error, Octocrab};

    enum RepoType {
        Python,
        Node,
    }

    struct RepoAnalysis<'a> {
        language: RepoType,
        file_to_detect: &'a str,
        file_content_update_fn: fn(content: String, version: &String) -> String,
    }

    const FILE_TO_LANGUAGE: [RepoAnalysis; 2] = [
        RepoAnalysis {
            file_to_detect: "package.json",
            language: RepoType::Node,
            file_content_update_fn: |content, version| {
                let re = Regex::new(r#""version":.+\n"#).unwrap();
                return re
                    .replace(&content, format!(r#""version": "{}"\n"#, version))
                    .to_string();
            },
        },
        RepoAnalysis {
            file_to_detect: "pyproject.toml",
            language: RepoType::Python,
            file_content_update_fn: |content, version| {
                let re = Regex::new(r"version\s=.+\n").unwrap();
                return re
                    .replace(&content, format!(r#"version = "{}"\n"#, version))
                    .to_string();
            },
        },
    ];

    fn get_repo_with_file_to_update<'a>(
        files: &Vec<octocrab::models::repos::Content>,
        version: &'a String,
    ) -> Option<(RepoAnalysis<'a>, String, String)> {
        for item in files {
            for analysis in FILE_TO_LANGUAGE {
                if item.name.eq(analysis.file_to_detect) {
                    let new_content =
                        (analysis.file_content_update_fn)(item.decoded_content().unwrap(), version);
                    return Some((analysis, new_content, item.sha.clone()));
                }
            }
        }
        return None;
    }

    pub fn create_octocrab(pat: &str) -> Result<Octocrab, Error> {
        let octocrab = Octocrab::builder().personal_token(pat).build()?;
        return Ok(octocrab);
    }

    pub async fn get_root_file_list(
        octocrab: &Arc<RwLock<Octocrab>>,
        owner: &String,
        repo: &String,
    ) -> Result<Vec<octocrab::models::repos::Content>, Error> {
        let content = octocrab
            .read()
            .await
            .repos(owner, repo)
            .get_content()
            .send()
            .await?;
        return Ok(content.items);
    }

    async fn create_pr(
        octocrab: &Arc<RwLock<Octocrab>>,
        owner: &String,
        repo: &String,
        title: &String,
        origin: &String,
        target: &String,
        body: &String,
    ) -> Result<(u64, String), Error> {
        let pr_result = octocrab
            .read()
            .await
            .pulls(owner, repo)
            .create(title, origin, target)
            .body(body)
            .send()
            .await?;

        return Ok((pr_result.number, pr_result.head.sha));
    }

    pub async fn update_file_version(
        octocrab: &Arc<RwLock<Octocrab>>,
        owner: &String,
        repo: &String,
        path: &str,
        content: &String,
        sha: &String,
        branch: &String,
    ) -> Result<(), Error> {
        octocrab
            .read()
            .await
            .repos(owner, repo)
            .update_file(path, "Bumping version", content, sha)
            .branch(branch)
            .send()
            .await?;

        return Ok(());
    }

    pub async fn merge_branch(
        octocrab: &Arc<RwLock<Octocrab>>,
        owner: &String,
        repo: &String,
        pr_number: u64,
    ) -> Result<String, Error> {
        let res = octocrab
            .read()
            .await
            .pulls(owner, repo)
            .merge(pr_number)
            .send()
            .await?;
        return Ok(res.sha.unwrap());
    }

    pub async fn create_release(
        octocrab: &Arc<RwLock<Octocrab>>,
        owner: &String,
        repo: &String,
        version: &String,
        merge_sha: &String,
    ) -> Result<(), Error> {
        octocrab
            .read()
            .await
            .repos(owner, repo)
            .releases()
            .create(version)
            .target_commitish(merge_sha)
            .make_latest(octocrab::repos::releases::MakeLatest::True)
            .send()
            .await?;
        octocrab
            .read()
            .await
            .repos(owner, repo)
            .releases()
            .generate_release_notes(&version)
            .send()
            .await?;
        return Ok(());
    }

    pub async fn create_version_branch(
        octocrab: &Arc<RwLock<Octocrab>>,
        owner: &String,
        repo: &String,
        version: &String,
        sha: &String,
    ) -> Result<(), Error> {
        octocrab
            .read()
            .await
            .repos(owner, repo)
            .create_ref(
                &octocrab::params::repos::Reference::Branch(version.clone()),
                sha,
            )
            .await?;
        return Ok(());
    }

    async fn set_up_repo(
        json_repo: &JSONRepo,
        octocrab: Arc<RwLock<Octocrab>>,
        config: Arc<RwLock<Config>>,
        version: Arc<RwLock<String>>,
    ) -> Result<(), Error> {
        let files = get_root_file_list(&octocrab, &json_repo.owner, &json_repo.repo).await?;
        let version_s = version.read().await.to_string();
        let file_to_update = match get_repo_with_file_to_update(&files, &version_s) {
            Some(analysis) => analysis,
            None => {
                panic!("No versionning file found");
            }
        };
        let pr_resullt = create_pr(
            &octocrab,
            &json_repo.owner,
            &json_repo.repo,
            &config.read().await.pattern.title,
            &json_repo.origin,
            &json_repo.target,
            &config.read().await.pattern.body,
        )
        .await?;
        update_file_version(
            &octocrab,
            &json_repo.owner,
            &json_repo.repo,
            &file_to_update.0.file_to_detect,
            &file_to_update.1,
            &file_to_update.2,
            &json_repo.origin,
        )
        .await?;
        let merge_result =
            merge_branch(&octocrab, &json_repo.owner, &json_repo.repo, pr_resullt.0).await?;
        create_release(
            &octocrab,
            &json_repo.owner,
            &json_repo.repo,
            &version.read().await.to_string(),
            &merge_result,
        )
        .await?;
        create_version_branch(
            &octocrab,
            &json_repo.owner,
            &json_repo.repo,
            &version.read().await.to_string(),
            &pr_resullt.1,
        )
        .await?;
        println!(
            "Repo {} setup with new version {}",
            json_repo.repo,
            version.read().await
        );

        return Ok(());
    }

    pub async fn set_all_repos(
        octocrab: Octocrab,
        config: Config,
        version: String,
    ) -> Result<(), Error> {
        println!(
            "Authenticated as {}",
            octocrab.current().user().await.unwrap().url
        );
        let mut set = JoinSet::new();
        let octocrab_arc = Arc::new(RwLock::new(octocrab));
        let config_arc = Arc::new(RwLock::new(config));
        let version_arc = Arc::new(RwLock::new(version));

        let repos = config_arc.read().await.repositories.clone();
        for json_repo in repos {
            let octocrab_clone = Arc::clone(&octocrab_arc);
            let config_clone = Arc::clone(&config_arc);
            let version_clone = Arc::clone(&version_arc);
            set.spawn(async move {
                match set_up_repo(&json_repo, octocrab_clone, config_clone, version_clone).await {
                    Ok(_) => {
                        println!("Successfuly setup {}", json_repo.repo);
                    }
                    Err(error) => {
                        println!("Error {error:?}");
                    }
                };
            });
        }
        set.join_all().await;
        return Ok(());
    }
}
