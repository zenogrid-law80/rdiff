use super::{init, language};
use gpui_kit::{
    base::input::Rope,
    component::highlighter::{HighlightTheme, Language, LanguageRegistry, SyntaxHighlighter},
};

// Exercise real grammar loading and highlight queries, rather than just aliases.
const SAMPLES: &[(&str, &str, &str)] = &[
    (
        "view.astro",
        "astro",
        "---\nconst title = \"hello\";\n---\n<h1>{title}</h1>\n",
    ),
    (
        "script.sh",
        "bash",
        "#!/bin/bash\nprintf '%s\\n' \"hello\"\n",
    ),
    ("main.c", "c", "int main(void) { return 42; }\n"),
    (
        "CMakeLists.txt",
        "cmake",
        "cmake_minimum_required(VERSION 3.20)\nproject(Hello)\nset(MESSAGE \"hello\")\n",
    ),
    (
        "main.cpp",
        "cpp",
        "#include <string>\nint main() { std::string text = \"hello\"; return 42; }\n",
    ),
    (
        "Main.cs",
        "csharp",
        "class Demo { static string Hello() { return \"안녕\"; } }\n",
    ),
    ("style.css", "css", ".demo { color: red; margin: 4px; }\n"),
    (
        "change.patch",
        "diff",
        "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n",
    ),
    ("view.ejs", "ejs", "<h1><%= \"hello\" %></h1>\n"),
    (
        "main.ex",
        "elixir",
        "defmodule Demo do\n  def hello, do: \"hello\"\nend\n",
    ),
    ("view.html.erb", "erb", "<p><%= :hello %></p>\n"),
    (
        "main.go",
        "go",
        "package main\nfunc main() { println(\"hello\") }\n",
    ),
    (
        "query.graphql",
        "graphql",
        "query Demo { user(id: 1) { name } }\n",
    ),
    (
        "page.html",
        "html",
        "<!DOCTYPE html><div class=\"demo\">hello</div>\n",
    ),
    (
        "Main.java",
        "java",
        "class Demo { String hello() { return \"hello\"; } }\n",
    ),
    (
        "view.jsx",
        "javascript",
        "const Demo = () => <div title=\"hello\">{42}</div>;\n",
    ),
    (
        "comment.jsdoc",
        "jsdoc",
        "/** @param {string} name - Hello */\n",
    ),
    (
        "data.json",
        "json",
        "{\"message\": \"hello\", \"count\": 42}\n",
    ),
    (
        "Main.kt",
        "kotlin",
        "fun main() { val message: String = \"hello\" }\n",
    ),
    (
        "main.lua",
        "lua",
        "local message = \"hello\"\nprint(message)\n",
    ),
    ("Makefile", "make", "all:\n\t@echo \"hello\"\n"),
    ("README.md", "markdown", "# Hello\n\n**world** and `code`\n"),
    (
        "index.php",
        "php",
        "<?php\n$message = \"hello\";\necho $message;\n",
    ),
    (
        "schema.proto",
        "proto",
        "syntax = \"proto3\";\nmessage Demo { string name = 1; }\n",
    ),
    (
        "main.py",
        "python",
        "def hello(name: str):\n    return f\"hello {name}\"\n",
    ),
    (
        "main.rb",
        "ruby",
        "class Demo\n  def hello\n    \"hello\"\n  end\nend\n",
    ),
    (
        "main.rs",
        "rust",
        "fn main() { let message = \"hello\"; }\n",
    ),
    (
        "Main.scala",
        "scala",
        "object Demo { val greeting: String = \"hello\" }\n",
    ),
    (
        "query.sql",
        "sql",
        "SELECT name FROM users WHERE id = 42;\n",
    ),
    (
        "view.svelte",
        "svelte",
        "<script>let name = \"hello\";</script>\n<h1>{name}</h1>\n",
    ),
    (
        "Main.swift",
        "swift",
        "func hello() -> String { return \"hello\" }\n",
    ),
    ("config.toml", "toml", "message = \"hello\"\ncount = 42\n"),
    (
        "view.tsx",
        "tsx",
        "const Demo = (): JSX.Element => <div title=\"hello\">{42}</div>;\n",
    ),
    (
        "main.ts",
        "typescript",
        "function hello(name: string): string { return `hi ${name}`; }\n",
    ),
    ("config.yml", "yaml", "message: hello\ncount: 42\n"),
    (
        "main.zig",
        "zig",
        "const std = @import(\"std\");\npub fn main() void { std.debug.print(\"hello\", .{}); }\n",
    ),
];

#[test]
fn every_bundled_language_parses_and_colors_in_both_themes() {
    init();
    let mut exercised = std::collections::BTreeSet::new();
    for &(filename, expected, source) in SAMPLES {
        assert_eq!(language(filename), expected, "{filename}");
        let rope = Rope::from(source);
        let mut highlighter = SyntaxHighlighter::new(expected);
        assert!(
            highlighter.update(None, &rope, None),
            "{expected}: parse timed out"
        );
        let tree = highlighter
            .tree()
            .unwrap_or_else(|| panic!("{expected}: no parser/query"));
        assert!(
            !tree.root_node().has_error(),
            "{expected}: {}",
            tree.root_node().to_sexp()
        );
        for theme in [
            HighlightTheme::default_dark(),
            HighlightTheme::default_light(),
        ] {
            let styles = highlighter.styles(&(0..source.len()), &*theme);
            assert!(
                styles.iter().any(|(_, style)| style.color.is_some()),
                "{expected}: no syntax colors"
            );
            for (range, _) in styles {
                assert!(
                    range.start <= range.end && range.end <= source.len(),
                    "{expected}"
                );
                assert!(
                    source.is_char_boundary(range.start) && source.is_char_boundary(range.end),
                    "{expected}"
                );
            }
        }
        exercised.insert(expected);
    }
    exercised.extend(["text", "markdown_inline"]);
    let bundled = Language::all()
        .map(|language| language.name())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        exercised, bundled,
        "Every GPUI Kit language needs a representative sample"
    );
}

#[test]
fn extensions_special_filenames_and_unknown_files_are_resolved() {
    for (path, expected) in [
        ("C:\\한글 폴더\\MAIN.PY", "python"),
        ("/repo/types.d.ts", "typescript"),
        ("main.MTS", "typescript"),
        ("entry.cjs", "javascript"),
        ("View.TSX", "tsx"),
        ("program.csx", "csharp"),
        ("build.KTS", "kotlin"),
        ("lib.HPP", "cpp"),
        ("config.jsonc", "json"),
        ("page.PHTML", "php"),
        ("query.gql", "graphql"),
        ("source.exs", "elixir"),
        ("project.sc", "scala"),
        ("build.zon", "zig"),
        ("module.cmake", "cmake"),
        ("/repo/GNUmakefile", "make"),
        ("rules.mk", "make"),
        ("C:\\src\\CMakeLists.txt", "cmake"),
        ("Gemfile", "ruby"),
        ("package.gemspec", "ruby"),
        (".bashrc", "bash"),
        (".profile", "bash"),
        ("Cargo.lock", "toml"),
        ("uv.lock", "toml"),
        (".clang-format", "yaml"),
        ("image.png", "text"),
        ("notes.txt", "text"),
        ("unknown", "text"),
        ("file.unknown", "text"),
        ("", "text"),
    ] {
        assert_eq!(language(path), expected, "{path}");
    }
}

#[test]
fn supplemental_queries_color_real_tokens_after_reparsing() {
    init();
    for (language, source, tokens) in [
        ("cpp", "int value() { return 42; }", ["int", "return", "42"]),
        (
            "csharp",
            "class Demo { string value = \"hello\"; }",
            ["class", "string", "\"hello\""],
        ),
        (
            "graphql",
            "query Demo { user(id: 42) { name } }",
            ["query", "42", "name"],
        ),
        (
            "proto",
            "syntax = \"proto3\"; message Demo { string name = 1; }",
            ["message", "\"proto3\"", "name"],
        ),
        (
            "swift",
            "func hello() -> String { return \"hello\" }",
            ["func", "return", "\"hello\""],
        ),
        (
            "tsx",
            "const Demo = () => <div title=\"hello\">{42}</div>;",
            ["const", "div", "\"hello\""],
        ),
        (
            "javascript",
            "const Demo = () => <div title=\"hello\">{42}</div>;",
            ["const", "div", "\"hello\""],
        ),
    ] {
        let mut highlighter = SyntaxHighlighter::new(language);
        let comment = if language == "graphql" {
            "# edited"
        } else {
            "// edited"
        };
        for text in [source.to_owned(), format!("{comment}\n{source}")] {
            assert!(
                highlighter.update(None, &Rope::from(text.as_str()), None),
                "{language}"
            );
            let styles = highlighter.styles(&(0..text.len()), &*HighlightTheme::default_dark());
            for token in tokens {
                let start = text.find(token).unwrap();
                assert!(
                    styles.iter().any(|(range, style)| range.start <= start
                        && start < range.end
                        && style.color.is_some()),
                    "{language}: {token} has no color"
                );
            }
        }
    }
}

#[test]
fn erb_uses_ruby_injections_and_inline_markdown_remains_available() {
    init();
    let registry = LanguageRegistry::singleton();
    assert!(
        registry
            .language("erb")
            .unwrap()
            .injections
            .contains("ruby")
    );
    let mut highlighter = SyntaxHighlighter::new("markdown_inline");
    let source = "**bold** and `code`";
    assert!(highlighter.update(None, &Rope::from(source), None));
    assert!(
        highlighter
            .styles(&(0..source.len()), &*HighlightTheme::default_dark())
            .iter()
            .any(|(_, style)| style.color.is_some())
    );
}
