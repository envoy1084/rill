//! A file's language, by its extension: for facets and filters.

/// The language `path` is written in, or `Other`.
pub fn language_of(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("rs") => "Rust",
        Some("ts" | "tsx" | "mts" | "cts") => "TypeScript",
        Some("js" | "jsx" | "mjs" | "cjs") => "JavaScript",
        Some("py" | "pyi") => "Python",
        Some("go") => "Go",
        Some("java") => "Java",
        Some("kt" | "kts") => "Kotlin",
        Some("c" | "h") => "C",
        Some("cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx") => "C++",
        Some("cs") => "C#",
        Some("rb") => "Ruby",
        Some("php") => "PHP",
        Some("swift") => "Swift",
        Some("scala") => "Scala",
        Some("sh" | "bash" | "zsh") => "Shell",
        Some("proto") => "Protobuf",
        Some("sql") => "SQL",
        Some("md" | "markdown") => "Markdown",
        Some("toml") => "TOML",
        Some("yaml" | "yml") => "YAML",
        Some("json") => "JSON",
        Some("html" | "htm") => "HTML",
        Some("css" | "scss") => "CSS",
        Some("cedar") => "Cedar",
        _ if name == "Dockerfile" => "Dockerfile",
        _ if name == "Makefile" => "Makefile",
        _ => "Other",
    }
}
