use clap::{Parser, Subcommand};
use anyhow::{Result, Context};
use serde::{Serialize, Deserialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use walkdir::WalkDir;

const JSON_FILE: &str = ".git_projects.json";

#[derive(Parser)]
#[command(name = "pm-tool")]
#[command(about = "批量管理当前目录下所有 git 项目（scan/clone/grep/pull）", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 扫描所有 git 项目并生成 json
    Scan,
    /// 根据 json 批量 clone
    Clone,
    /// 在所有仓库执行 git grep
    Grep {
        /// 搜索模式
        pattern: String,
    },
    /// 在所有仓库执行 git pull
    Pull,
}

#[derive(Serialize, Deserialize)]
struct RepoRemote {
    remote: String,
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Scan => scan_git_projects(),
        Commands::Clone => clone_from_json(),
        Commands::Grep { pattern } => grep_all_projects(&pattern),
        Commands::Pull => pull_all_projects(),
    }
}

fn find_git_dirs() -> Vec<PathBuf> {
    WalkDir::new(".")
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir() && e.file_name() == ".git")
        .map(|e| e.path().parent().unwrap().to_path_buf())
        .collect()
}

fn scan_git_projects() -> Result<()> {
    let mut repos = Vec::new();
    for repo_dir in find_git_dirs() {
        let output = Command::new("git")
            .arg("remote")
            .arg("get-url")
            .arg("origin")
            .current_dir(&repo_dir)
            .output()
            .ok();

        if let Some(out) = output {
            if out.status.success() {
                let remote = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !remote.is_empty() {
                    repos.push(RepoRemote { remote });
                }
            }
        }
    }
    let file = File::create(JSON_FILE)?;
    serde_json::to_writer_pretty(file, &repos)?;
    println!("已生成 {}", JSON_FILE);
    Ok(())
}

fn clone_from_json() -> Result<()> {
    let data = fs::read_to_string(JSON_FILE)
        .with_context(|| format!("请先执行 scan，未找到 {}", JSON_FILE))?;
    let repos: Vec<RepoRemote> = serde_json::from_str(&data)?;
    for repo in repos {
        let (repo_path, _) = parse_repo_path(&repo.remote)?;
        if repo_path.exists() {
            println!("目录 {:?} 已存在，跳过。", repo_path);
            continue;
        }
        if let Some(parent) = repo_path.parent() {
            fs::create_dir_all(parent)?;
        }
        println!("正在 clone {} 到 {:?}", repo.remote, repo_path);
        Command::new("git")
            .arg("clone")
            .arg(&repo.remote)
            .arg(&repo_path)
            .status()
            .with_context(|| format!("git clone {} 失败", repo.remote))?;
    }
    Ok(())
}

fn parse_repo_path(remote: &str) -> Result<(PathBuf, String)> {
    if remote.starts_with("git@") {
        let repo_path = remote
            .split(':')
            .nth(1)
            .and_then(|s| s.strip_suffix(".git"))
            .ok_or_else(|| anyhow::anyhow!("无法解析仓库路径: {}", remote))?;
        Ok((PathBuf::from(repo_path), repo_path.to_string()))
    } else if remote.starts_with("http") {
        let repo_path = remote
            .split('/')
            .skip(3)
            .collect::<Vec<_>>()
            .join("/")
            .strip_suffix(".git")
            .ok_or_else(|| anyhow::anyhow!("无法解析仓库路径: {}", remote))?
            .to_string();
        Ok((PathBuf::from(&repo_path), repo_path))
    } else {
        Err(anyhow::anyhow!("无法解析仓库路径: {}", remote))
    }
}

fn grep_all_projects(pattern: &str) -> Result<()> {
    for repo_dir in find_git_dirs() {
        println!("Processing Git repository in {:?}", repo_dir);
        let output = Command::new("git")
            .arg("grep")
            .arg(pattern)
            .arg("--all-match")
            .arg("--break")
            .arg("--heading")
            .arg("--line-number")
            .arg("--color")
            .arg("$(git rev-list --all)")
            .current_dir(&repo_dir)
            .output()
            .with_context(|| format!("在 {:?} 执行 grep 出错", repo_dir))?;
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
    Ok(())
}

fn pull_all_projects() -> Result<()> {
    for repo_dir in find_git_dirs() {
        println!("Processing Git repository in {:?}", repo_dir);
        let status = Command::new("git")
            .arg("status")
            .arg("--porcelain")
            .current_dir(&repo_dir)
            .output()?;
        if !status.stdout.is_empty() {
            Command::new("git").arg("add").arg(".").current_dir(&repo_dir).status()?;
            Command::new("git").arg("stash").current_dir(&repo_dir).status()?;
        }
        Command::new("git").arg("checkout").arg("master").current_dir(&repo_dir).status()?;
        Command::new("git").arg("pull").arg("origin").arg("master").current_dir(&repo_dir).status()?;
    }
    Ok(())
}
