use gpui_kit::component::highlighter::LanguageRegistry;
use std::sync::Once;

/// Enable complete highlight queries before constructing any editors.
pub fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let registry = LanguageRegistry::singleton();
        for (name, highlights) in [
            ("csharp", tree_sitter_c_sharp::HIGHLIGHTS_QUERY),
            ("cmake", tree_sitter_cmake::HIGHLIGHTS_QUERY),
            ("swift", tree_sitter_swift::HIGHLIGHTS_QUERY),
            ("graphql", include_str!("syntax/graphql.scm")),
            ("proto", include_str!("syntax/proto.scm")),
        ] {
            let mut config = registry.language(name).expect("bundled grammar");
            config.highlights = highlights.to_owned().into();
            registry.register(name, &config);
        }

        // The C++ query contains only C++ additions; inherit the shared C rules.
        let mut cpp = registry.language("cpp").expect("C++ grammar");
        cpp.highlights = format!(
            "{}\n{}",
            registry.language("c").expect("C grammar").highlights,
            cpp.highlights
        )
        .into();
        registry.register("cpp", &cpp);

        // TSX needs JavaScript/TypeScript token rules as well as JSX tags.
        let typescript = registry.language("typescript").expect("TypeScript grammar");
        for name in ["javascript", "tsx"] {
            let mut config = registry.language(name).expect("JSX-capable grammar");
            let base = if name == "tsx" {
                &typescript.highlights
            } else {
                &config.highlights
            };
            config.highlights =
                format!("{}\n{}", base, tree_sitter_javascript::JSX_HIGHLIGHT_QUERY).into();
            registry.register(name, &config);
        }

        // GPUI's ERB default points to the EJS injection query.
        let mut erb = registry.language("erb").expect("ERB grammar");
        erb.injections = tree_sitter_embedded_template::INJECTIONS_ERB_QUERY.into();
        registry.register("erb", &erb);
    });
}

/// Resolve a filename without reading or changing the file's contents.
pub fn language(path: &str) -> &'static str {
    // Accept both Windows and Unix separators, including paths compared on macOS.
    let name = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase();
    match name.as_str() {
        "makefile" | "gnumakefile" | "bsdmakefile" => return "make",
        "cmakelists.txt" => return "cmake",
        "gemfile" | "rakefile" | "guardfile" | "podfile" | "vagrantfile" | "brewfile"
        | ".irbrc" | ".pryrc" => return "ruby",
        ".bashrc" | ".bash_profile" | ".bash_login" | ".bash_logout" | ".bash_aliases"
        | ".profile" => return "bash",
        "cargo.lock" | "poetry.lock" | "uv.lock" | "pipfile" => return "toml",
        ".clang-format" | ".clang-tidy" => return "yaml",
        _ => {}
    }
    match name
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .unwrap_or("")
    {
        "astro" => "astro",
        "sh" | "bash" => "bash",
        "c" | "h" => "c",
        "cmake" => "cmake",
        "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" | "ipp" | "inl" | "tpp"
        | "ixx" | "cppm" => "cpp",
        "cs" | "csx" => "csharp",
        "css" | "scss" => "css",
        "diff" | "patch" => "diff",
        "ejs" => "ejs",
        "ex" | "exs" => "elixir",
        "erb" => "erb",
        "go" => "go",
        "graphql" | "graphqls" | "gql" => "graphql",
        "html" | "htm" | "xhtml" | "shtml" => "html",
        "java" => "java",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "jsdoc" => "jsdoc",
        "json" | "jsonc" => "json",
        "kt" | "kts" | "ktm" => "kotlin",
        "lua" => "lua",
        "mk" | "mak" | "make" => "make",
        "md" | "markdown" | "mdown" | "mkd" | "mkdn" | "mdx" => "markdown",
        "php" | "php3" | "php4" | "php5" | "php7" | "php8" | "phtml" => "php",
        "proto" => "proto",
        "py" | "pyw" | "pyi" => "python",
        "rb" | "rake" | "gemspec" | "ru" => "ruby",
        "rs" => "rust",
        "scala" | "sc" => "scala",
        "sql" | "ddl" | "dml" => "sql",
        "svelte" => "svelte",
        "swift" => "swift",
        "toml" => "toml",
        "tsx" => "tsx",
        "ts" | "mts" | "cts" => "typescript",
        "yaml" | "yml" => "yaml",
        "zig" | "zon" => "zig",
        _ => "text",
    }
}

#[cfg(test)]
mod tests;
