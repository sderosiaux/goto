use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::db::Database;
use crate::embedding::{embed_text, embed_texts};

/// Maximum characters to read from README
const README_MAX_CHARS: usize = 1500;

/// Metadata extracted from a project
#[derive(Debug, Default)]
pub struct ProjectMetadata {
    pub description: Option<String>,
    pub readme_excerpt: Option<String>,
    pub tech_stack: Vec<String>,
    pub keywords: Vec<String>,
}

impl ProjectMetadata {
    /// Build the text to be embedded
    pub fn to_embedding_text(&self, project_name: &str) -> String {
        let mut parts = vec![project_name.to_string()];

        if let Some(desc) = &self.description {
            parts.push(desc.clone());
        }

        if !self.keywords.is_empty() {
            parts.push(self.keywords.join(", "));
        }

        if let Some(readme) = &self.readme_excerpt {
            parts.push(readme.clone());
        }

        if !self.tech_stack.is_empty() {
            parts.push(format!("Technologies: {}", self.tech_stack.join(", ")));
        }

        parts.join(" | ")
    }
}

/// Extract metadata from a project directory
pub fn extract_metadata(path: &Path) -> ProjectMetadata {
    let mut meta = ProjectMetadata::default();

    // Try to read description from package.json (Node.js)
    if let Some(desc) = read_package_json_description(path) {
        meta.description = Some(desc);
    }

    // Try to read description from Cargo.toml (Rust)
    if meta.description.is_none() {
        if let Some(desc) = read_cargo_toml_description(path) {
            meta.description = Some(desc);
        }
    }

    // Try to read description from pyproject.toml (Python)
    if meta.description.is_none() {
        if let Some(desc) = read_pyproject_description(path) {
            meta.description = Some(desc);
        }
    }

    // Read README excerpt
    meta.readme_excerpt = read_readme_excerpt(path);

    // Detect tech stack
    meta.tech_stack = detect_tech_stack(path);

    // Read keywords from Cargo.toml or package.json
    meta.keywords = read_cargo_keywords(path)
        .or_else(|| read_package_json_keywords(path))
        .unwrap_or_default();

    meta
}

/// Detect technologies used in the project
fn detect_tech_stack(path: &Path) -> Vec<String> {
    let mut stack = Vec::new();

    // (build file, extension, language name)
    let markers: &[(&str, &str, &str)] = &[
        // Systems
        ("Cargo.toml", ".rs", "Rust"),
        ("CMakeLists.txt", ".c", "C"),
        ("CMakeLists.txt", ".cpp", "C++"),
        ("meson.build", ".c", "C"),
        ("build.zig", ".zig", "Zig"),
        // JVM
        ("pom.xml", ".java", "Java"),
        ("build.gradle", ".java", "Java"),
        ("build.gradle.kts", ".kt", "Kotlin"),
        ("build.sbt", ".scala", "Scala"),
        ("project.clj", ".clj", "Clojure"),
        // Web
        ("package.json", ".js", "JavaScript"),
        ("tsconfig.json", ".ts", "TypeScript"),
        ("deno.json", ".ts", "Deno"),
        ("bun.lockb", ".ts", "Bun"),
        // Python
        ("pyproject.toml", ".py", "Python"),
        ("requirements.txt", ".py", "Python"),
        ("setup.py", ".py", "Python"),
        ("Pipfile", ".py", "Python"),
        // Go
        ("go.mod", ".go", "Go"),
        // Ruby
        ("Gemfile", ".rb", "Ruby"),
        // PHP
        ("composer.json", ".php", "PHP"),
        // Elixir/Erlang
        ("mix.exs", ".ex", "Elixir"),
        ("rebar.config", ".erl", "Erlang"),
        // Functional
        ("stack.yaml", ".hs", "Haskell"),
        ("dune-project", ".ml", "OCaml"),
        // Mobile
        ("Package.swift", ".swift", "Swift"),
        ("Podfile", ".swift", "iOS"),
        ("build.gradle", ".kt", "Android"),
        // .NET (checked via extension scan below)
        // Infra
        ("Dockerfile", "", "Docker"),
        ("docker-compose.yml", "", "Docker"),
        ("docker-compose.yaml", "", "Docker"),
        ("terraform.tf", ".tf", "Terraform"),
        ("main.tf", ".tf", "Terraform"),
        ("serverless.yml", "", "Serverless"),
        ("pulumi.yaml", "", "Pulumi"),
        ("kubernetes.yaml", "", "Kubernetes"),
        // Frameworks
        ("next.config.js", "", "Next.js"),
        ("next.config.mjs", "", "Next.js"),
        ("nuxt.config.ts", "", "Nuxt"),
        ("vite.config.ts", "", "Vite"),
        ("astro.config.mjs", "", "Astro"),
        ("svelte.config.js", "", "Svelte"),
        ("angular.json", "", "Angular"),
        ("tailwind.config.js", "", "Tailwind"),
        ("tailwind.config.ts", "", "Tailwind"),
        // Data
        ("dbt_project.yml", ".sql", "dbt"),
        // Other
        ("Makefile", "", "Make"),
        ("justfile", "", "Just"),
        ("Taskfile.yml", "", "Task"),
    ];

    for (file, _, tech) in markers {
        if path.join(file).exists() && !stack.contains(&tech.to_string()) {
            stack.push(tech.to_string());
        }
    }

    // Additional extension mappings (for src scan)
    let ext_map: &[(&str, &str)] = &[
        (".rs", "Rust"),
        (".ts", "TypeScript"),
        (".tsx", "TypeScript"),
        (".js", "JavaScript"),
        (".jsx", "JavaScript"),
        (".py", "Python"),
        (".go", "Go"),
        (".java", "Java"),
        (".kt", "Kotlin"),
        (".scala", "Scala"),
        (".rb", "Ruby"),
        (".php", "PHP"),
        (".ex", "Elixir"),
        (".exs", "Elixir"),
        (".hs", "Haskell"),
        (".ml", "OCaml"),
        (".swift", "Swift"),
        (".cs", "C#"),
        (".fs", "F#"),
        (".c", "C"),
        (".cpp", "C++"),
        (".cc", "C++"),
        (".zig", "Zig"),
        (".lua", "Lua"),
        (".clj", "Clojure"),
        (".erl", "Erlang"),
        (".tf", "Terraform"),
        (".vue", "Vue"),
        (".svelte", "Svelte"),
    ];

    // Check src directory and root for extensions
    for dir in [path, &path.join("src"), &path.join("lib"), &path.join("app")] {
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.take(30).flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                for (ext, tech) in ext_map {
                    if name.ends_with(ext) && !stack.contains(&tech.to_string()) {
                        stack.push(tech.to_string());
                        break;
                    }
                }
            }
        }
    }

    stack
}

/// Read description from package.json
fn read_package_json_description(path: &Path) -> Option<String> {
    let pkg_path = path.join("package.json");
    let content = fs::read_to_string(pkg_path).ok()?;
    extract_json_string(&content, "description")
}

/// Read description from Cargo.toml
fn read_cargo_toml_description(path: &Path) -> Option<String> {
    let cargo_path = path.join("Cargo.toml");
    let content = fs::read_to_string(cargo_path).ok()?;
    let value: toml::Value = content.parse().ok()?;
    // Try [package] first, then [workspace.package]
    value.get("package")
        .or_else(|| value.get("workspace")?.get("package"))?
        .get("description")?
        .as_str()
        .map(String::from)
}

/// Read keywords from Cargo.toml
fn read_cargo_keywords(path: &Path) -> Option<Vec<String>> {
    let cargo_path = path.join("Cargo.toml");
    let content = fs::read_to_string(cargo_path).ok()?;
    let value: toml::Value = content.parse().ok()?;
    // Try [package] first, then [workspace.package]
    let pkg = value.get("package")
        .or_else(|| value.get("workspace")?.get("package"))?;
    let keywords = pkg.get("keywords")?.as_array()?;
    let result: Vec<String> = keywords
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    if result.is_empty() { None } else { Some(result) }
}

/// Read keywords from package.json
fn read_package_json_keywords(path: &Path) -> Option<Vec<String>> {
    let pkg_path = path.join("package.json");
    let content = fs::read_to_string(pkg_path).ok()?;
    // Simple extraction - look for "keywords": [...]
    let start = content.find("\"keywords\"")?;
    let after = &content[start..];
    let arr_start = after.find('[')?;
    let arr_end = after.find(']')?;
    let arr_content = &after[arr_start + 1..arr_end];
    let keywords: Vec<String> = arr_content
        .split(',')
        .filter_map(|s| {
            let trimmed = s.trim().trim_matches('"');
            if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
        })
        .collect();
    if keywords.is_empty() { None } else { Some(keywords) }
}

/// Read description from pyproject.toml
fn read_pyproject_description(path: &Path) -> Option<String> {
    let py_path = path.join("pyproject.toml");
    let content = fs::read_to_string(py_path).ok()?;
    let value: toml::Value = content.parse().ok()?;

    // Try [project] section first (PEP 621), then poetry
    let project = value.get("project")
        .or_else(|| value.get("tool")?.get("poetry"))?;

    project.get("description")?
        .as_str()
        .map(String::from)
}

/// Read first paragraph from README
fn read_readme_excerpt(path: &Path) -> Option<String> {
    let readme_names = ["README.md", "README", "readme.md", "Readme.md"];

    for name in readme_names {
        let readme_path = path.join(name);
        if let Ok(content) = fs::read_to_string(&readme_path) {
            return Some(extract_first_paragraph(&content));
        }
    }

    None
}

/// Extract meaningful content from markdown README
fn extract_first_paragraph(content: &str) -> String {
    // First, strip all HTML tags
    let stripped = strip_html_tags(content);

    let mut result = String::new();

    for line in stripped.lines() {
        let trimmed = line.trim();

        // Skip empty or short lines
        if trimmed.len() < 10 {
            continue;
        }

        // Skip non-content lines
        if trimmed.starts_with('#')           // headers
            || trimmed.starts_with('[')       // badges/links
            || trimmed.starts_with('!')       // images
            || trimmed.starts_with("```")     // code blocks
            || trimmed.starts_with("<!--")    // comments
            || trimmed.starts_with("* ")      // bullet points (with space)
            || trimmed.starts_with("- ")      // bullet points (with space)
            || trimmed.contains("shields.io") // badges
        {
            continue;
        }

        // Add content
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(trimmed);

        if result.len() >= README_MAX_CHARS {
            break;
        }
    }

    // Truncate to max chars (UTF-8 safe)
    if result.len() > README_MAX_CHARS {
        // Find a safe truncation point (char boundary)
        let mut end = README_MAX_CHARS;
        while !result.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        result.truncate(end);
        // Try to end at a word boundary
        if let Some(last_space) = result.rfind(' ') {
            result.truncate(last_space);
        }
        result.push_str("...");
    }

    result
}

/// Strip HTML tags from content
fn strip_html_tags(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut in_tag = false;

    for c in content.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }

    result
}

// Simple JSON extraction without serde_json
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let start = json.find(&pattern)?;
    let after_key = &json[start + pattern.len()..];

    // Skip whitespace and colon
    let value_start = after_key.find('"')? + 1;
    let rest = &after_key[value_start..];
    let value_end = rest.find('"')?;

    Some(rest[..value_end].to_string())
}


/// Index all unindexed projects
pub fn index_projects(db: &Database) -> Result<usize> {
    let unindexed = db.get_unindexed_projects()?;

    if unindexed.is_empty() {
        return Ok(0);
    }

    eprintln!(
        "\x1b[36m⏳\x1b[0m Indexing {} projects semantically...",
        unindexed.len()
    );

    // Extract metadata and build texts for embedding
    let mut texts: Vec<String> = Vec::with_capacity(unindexed.len());
    let mut project_data: Vec<(i64, ProjectMetadata)> = Vec::with_capacity(unindexed.len());

    for (id, path, name) in &unindexed {
        let meta = extract_metadata(path);
        let text = meta.to_embedding_text(name);
        texts.push(text);
        project_data.push((*id, meta));
    }

    // Generate embeddings in batch
    let embeddings = embed_texts(&texts)?;

    // Store in database
    for ((id, meta), (embedding, text)) in project_data.iter().zip(embeddings.iter().zip(texts.iter())) {
        db.upsert_metadata(
            *id,
            meta.description.as_deref(),
            meta.readme_excerpt.as_deref(),
            text,
        )?;

        db.upsert_embedding(*id, embedding)?;
    }

    Ok(unindexed.len())
}

/// Perform semantic search
pub fn semantic_search(db: &Database, query: &str, limit: usize) -> Result<Vec<(crate::db::Project, f32)>> {
    // Embed the query
    let query_embedding = embed_text(query)?;

    // Find similar projects
    let similar = db.find_similar(&query_embedding, limit)?;

    // Convert to projects with scores
    let mut results = Vec::with_capacity(similar.len());
    for (project_id, distance) in similar {
        if let Some(project) = db.get_project_by_id(project_id)? {
            // Convert distance to similarity score (0-100)
            // sqlite-vec uses L2 distance, so we need to convert
            let similarity = (1.0 / (1.0 + distance)) * 100.0;
            results.push((project, similarity));
        }
    }

    Ok(results)
}
