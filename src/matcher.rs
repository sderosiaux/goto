use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::db::Project;

pub struct Matcher {
    matcher: SkimMatcherV2,
}

#[derive(Debug)]
pub struct MatchResult<'a> {
    pub project: &'a Project,
    pub fuzzy_score: i64,
}

impl Matcher {
    pub fn new() -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
        }
    }

    /// Find projects matching the query, sorted by combined score
    /// Returns references to avoid cloning
    pub fn find_matches<'a>(&self, query: &str, projects: &'a [Project]) -> Vec<MatchResult<'a>> {
        let mut matches: Vec<MatchResult<'a>> = projects
            .iter()
            .filter_map(|project| {
                // Try matching against project name first (higher weight)
                let name_score = self.matcher.fuzzy_match(&project.name, query);

                // Also try matching against the full path
                let path_str = project.path.to_string_lossy();
                let path_score = self.matcher.fuzzy_match(&path_str, query);

                // Take the better of the two scores
                let fuzzy_score = match (name_score, path_score) {
                    (Some(n), Some(p)) => Some(n.max(p)),
                    (Some(n), None) => Some(n),
                    (None, Some(p)) => Some(p),
                    (None, None) => None,
                }?;

                Some(MatchResult {
                    project,
                    fuzzy_score,
                })
            })
            .collect();

        // Sort by: 1) fuzzy score (higher first), 2) recency (more recent first)
        matches.sort_unstable_by(|a, b| {
            match b.fuzzy_score.cmp(&a.fuzzy_score) {
                std::cmp::Ordering::Equal => b.project.last_accessed.cmp(&a.project.last_accessed),
                other => other,
            }
        });

        matches
    }
}

impl Default for Matcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::path::PathBuf;
    use crate::db::ProjectSource;

    fn make_project(name: &str, path: &str, access_count: i64) -> Project {
        Project {
            id: 1,
            name: name.to_string(),
            path: PathBuf::from(path),
            last_accessed: Utc::now(),
            access_count,
            last_modified: Utc::now(),
            source: ProjectSource::Scan,
        }
    }

    #[test]
    fn test_fuzzy_match() {
        let matcher = Matcher::new();
        let projects = vec![
            make_project("my-docs", "/home/user/projects/my-docs", 5),
            make_project("api-docs", "/home/user/work/api-docs", 2),
            make_project("documentation", "/home/user/documentation", 10),
        ];

        let matches = matcher.find_matches("docs", &projects);
        assert!(!matches.is_empty());

        // All three should match "docs"
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn test_no_clone_overhead() {
        let matcher = Matcher::new();
        let projects = vec![
            make_project("project1", "/path/to/project1", 1),
            make_project("project2", "/path/to/project2", 2),
        ];

        let matches = matcher.find_matches("project", &projects);

        // Verify we're getting references, not clones
        assert_eq!(matches[0].project.name, projects[0].name);
        assert!(std::ptr::eq(
            matches[0].project as *const _,
            &projects[0] as *const _
        ) || std::ptr::eq(
            matches[0].project as *const _,
            &projects[1] as *const _
        ));
    }
}
