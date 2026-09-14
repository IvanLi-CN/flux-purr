use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
};

use syn::{
    Attribute, File, FnArg, ImplItemFn, Item, ItemFn, ItemImpl, ItemMod, ItemTrait, Meta,
    Signature, Token, TraitItemFn, parse::Parser, punctuated::Punctuated, spanned::Spanned,
    visit::Visit,
};

const FUNCTION_LINE_LIMIT: usize = 100;
const ARGUMENT_LIMIT: usize = 7;
const NESTING_LIMIT: usize = 4;
const ENTRY_LINE_LIMIT: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Violation {
    path: PathBuf,
    line: usize,
    message: String,
}

#[derive(Default)]
struct FunctionMetrics {
    nesting: usize,
    max_nesting: usize,
}

impl<'ast> Visit<'ast> for FunctionMetrics {
    fn visit_block(&mut self, node: &'ast syn::Block) {
        self.nesting += 1;
        self.max_nesting = self.max_nesting.max(self.nesting);
        syn::visit::visit_block(self, node);
        self.nesting -= 1;
    }
}

struct SourceVisitor<'a> {
    path: &'a Path,
    source: &'a str,
    violations: Vec<Violation>,
}

fn has_local_expectation(attrs: &[Attribute], lint: &str) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("expect")
            && attribute_details(attr).is_some_and(|details| details.lints.contains(&lint))
    })
}

#[derive(Debug, Default)]
struct AttributeDetails {
    lints: Vec<&'static str>,
    has_reason: bool,
}

fn attribute_details(attr: &Attribute) -> Option<AttributeDetails> {
    let list = attr.meta.require_list().ok()?;
    let metas = Punctuated::<Meta, Token![,]>::parse_terminated
        .parse2(list.tokens.clone())
        .ok()?;
    let mut details = AttributeDetails::default();
    let nested_metas: Vec<Meta> = if attr.path().is_ident("cfg_attr") {
        metas.into_iter().skip(1).collect()
    } else {
        metas.into_iter().collect()
    };
    for meta in nested_metas {
        if attr.path().is_ident("cfg_attr") {
            let Meta::List(nested) = meta else {
                continue;
            };
            if !nested.path.is_ident("allow") && !nested.path.is_ident("expect") {
                continue;
            }
            let nested_tokens = nested.tokens;
            let nested_metas = Punctuated::<Meta, Token![,]>::parse_terminated
                .parse2(nested_tokens)
                .ok()?;
            for nested_meta in nested_metas {
                collect_attribute_detail(nested_meta, &mut details);
            }
        } else {
            collect_attribute_detail(meta, &mut details);
        }
    }
    Some(details)
}

fn collect_attribute_detail(meta: Meta, details: &mut AttributeDetails) {
    match meta {
        Meta::Path(path) => {
            for lint in ["too_many_lines", "too_many_arguments", "excessive_nesting"] {
                if path
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == lint)
                    && path.segments.len() == 2
                    && path
                        .segments
                        .first()
                        .is_some_and(|segment| segment.ident == "clippy")
                {
                    details.lints.push(lint);
                }
            }
        }
        Meta::NameValue(name_value) if name_value.path.is_ident("reason") => {
            details.has_reason = true;
        }
        _ => {}
    }
}

impl<'a> SourceVisitor<'a> {
    fn check_function(&mut self, attrs: &[Attribute], signature: &Signature, body: &syn::Block) {
        let line = signature.ident.span().start().line;
        let lines = clippy_line_count(self.source, body);
        if lines > FUNCTION_LINE_LIMIT && !has_local_expectation(attrs, "too_many_lines") {
            self.push(
                line,
                format!(
                    "{} has {lines} lines (limit {FUNCTION_LINE_LIMIT})",
                    signature.ident
                ),
            );
        }

        let args = signature
            .inputs
            .iter()
            .filter(|arg| matches!(arg, FnArg::Typed(_)))
            .count();
        if args > ARGUMENT_LIMIT {
            self.push(
                line,
                format!(
                    "{} has {args} parameters (limit {ARGUMENT_LIMIT})",
                    signature.ident
                ),
            );
        }

        let mut metrics = FunctionMetrics::default();
        metrics.visit_block(body);
        let nesting = metrics.max_nesting.saturating_sub(1);
        if nesting > NESTING_LIMIT && !has_local_expectation(attrs, "excessive_nesting") {
            self.push(
                line,
                format!(
                    "{} nesting is {} (limit {NESTING_LIMIT})",
                    signature.ident, nesting
                ),
            );
        }
    }

    fn check_attributes(&mut self, attrs: &[Attribute], allow_local_structural: bool) {
        for attr in attrs {
            if !attr.path().is_ident("expect")
                && !attr.path().is_ident("allow")
                && !attr.path().is_ident("cfg_attr")
            {
                continue;
            }
            let Some(details) = attribute_details(attr) else {
                continue;
            };
            if attr.path().is_ident("cfg_attr") && !details.lints.is_empty() {
                for lint in details.lints {
                    self.push(
                        attr.path().span().start().line,
                        format!("cfg_attr structural {lint} exceptions are not permitted"),
                    );
                }
                continue;
            }
            if details.lints.contains(&"too_many_arguments") {
                self.push(
                    attr.path().span().start().line,
                    "structural too_many_arguments exceptions are not permitted".to_string(),
                );
            }
            for lint in details.lints.iter().copied() {
                if !allow_local_structural {
                    self.push(
                        attr.path().span().start().line,
                        format!("container-level {lint} exceptions are not permitted"),
                    );
                    continue;
                }
                if attr.path().is_ident("allow") && lint != "too_many_arguments" {
                    self.push(
                        attr.path().span().start().line,
                        format!("structural {lint} exceptions must use #[expect]"),
                    );
                }
                if lint != "too_many_arguments" && !details.has_reason {
                    self.push(
                        attr.path().span().start().line,
                        format!("{lint} exceptions must include a reason"),
                    );
                }
            }
        }
    }

    fn push(&mut self, line: usize, message: String) {
        self.violations.push(Violation {
            path: self.path.to_path_buf(),
            line,
            message,
        });
    }
}

fn clippy_line_count(source: &str, body: &syn::Block) -> usize {
    let start = body.brace_token.span.open().byte_range().start;
    let end = body.brace_token.span.close().byte_range().end;
    let Some(mut function_source) = source.get(start..end) else {
        return body
            .brace_token
            .span
            .close()
            .end()
            .line
            .saturating_sub(body.brace_token.span.open().start().line)
            .saturating_sub(1);
    };
    if function_source.as_bytes().first() == Some(&b'{')
        && function_source.as_bytes().last() == Some(&b'}')
    {
        function_source = &function_source[1..function_source.len() - 1];
    }

    let mut in_comment = false;
    let mut count = 0;
    for mut line in function_source.trim().lines() {
        let mut code_in_line = false;
        loop {
            line = line.trim_start();
            if line.is_empty() {
                break;
            }
            if in_comment {
                if let Some(index) = line.find("*/") {
                    line = &line[index + 2..];
                    in_comment = false;
                    continue;
                }
            } else {
                let block_index = line.find("/*").unwrap_or(line.len());
                let line_index = line.find("//").unwrap_or(line.len());
                code_in_line |= block_index > 0 && line_index > 0;
                if block_index < line_index {
                    line = &line[block_index + 2..];
                    in_comment = true;
                    continue;
                }
            }
            break;
        }
        if code_in_line {
            count += 1;
        }
    }
    count
}

impl<'ast> Visit<'ast> for SourceVisitor<'_> {
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.check_attributes(&node.attrs, false);
        syn::visit::visit_item_mod(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.check_attributes(&node.attrs, true);
        self.check_function(&node.attrs, &node.sig, &node.block);
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        self.check_attributes(&node.attrs, false);
        syn::visit::visit_item_impl(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {
        self.check_attributes(&node.attrs, false);
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        self.check_attributes(&node.attrs, true);
        self.check_function(&node.attrs, &node.sig, &node.block);
        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_trait_item_fn(&mut self, node: &'ast TraitItemFn) {
        self.check_attributes(&node.attrs, true);
        if let Some(body) = &node.default {
            self.check_function(&node.attrs, &node.sig, body);
        }
        syn::visit::visit_trait_item_fn(self, node);
    }
}

fn collect_rust_files(root: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    if root.is_file() {
        if root.extension().is_some_and(|ext| ext == "rs") {
            files.push(root.to_path_buf());
        }
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn entry_checks(root: &Path) -> io::Result<Vec<Violation>> {
    let entries = [
        root.join("firmware/src/bin/flux_purr.rs"),
        root.join("tools/flux-purr-devd/src/bin/flux-purr.rs"),
        root.join("tools/flux-purr-devd/src/lib.rs"),
    ];
    let mut violations = Vec::new();
    for path in entries {
        let source = fs::read_to_string(&path)?;
        let lines = source.lines().count();
        if lines > ENTRY_LINE_LIMIT {
            violations.push(Violation {
                path: path.clone(),
                line: ENTRY_LINE_LIMIT + 1,
                message: format!("entry has {lines} lines (limit {ENTRY_LINE_LIMIT})"),
            });
        }
        let syntax = syn::parse_file(&source).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {error}", path.display()),
            )
        })?;
        if has_inline_tests(&syntax) {
            violations.push(Violation {
                path,
                line: 1,
                message: "entry must declare tests in a separate module file".to_string(),
            });
        }
    }
    Ok(violations)
}

fn has_inline_tests(file: &File) -> bool {
    fn module_is_tests(module: &ItemMod) -> bool {
        module.ident == "tests" && module.content.is_some()
    }
    file.items.iter().any(|item| match item {
        Item::Mod(module) => module_is_tests(module),
        _ => false,
    })
}

fn scan_sources(root: &Path) -> io::Result<Vec<Violation>> {
    let mut paths = Vec::new();
    for source_root in [
        root.join("firmware/src"),
        root.join("firmware/build.rs"),
        root.join("tools/flux-purr-devd/src"),
        root.join("tools/flux-purr-devd/build.rs"),
        root.join("tools/rust-style-check/src"),
    ] {
        collect_rust_files(&source_root, &mut paths)?;
    }
    paths.sort();
    let mut violations = entry_checks(root)?;
    for path in paths {
        let source = fs::read_to_string(&path)?;
        let syntax = syn::parse_file(&source).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {error}", path.display()),
            )
        })?;
        check_file_attributes(&path, &syntax, &mut violations);
        // syn parses cfg-gated code as source syntax, so the same structural
        // contract is enforced even when a host build cannot compile a target
        // branch. Clippy remains the target-aware semantic check.
        let mut visitor = SourceVisitor {
            path: &path,
            source: &source,
            violations: Vec::new(),
        };
        visitor.visit_file(&syntax);
        violations.extend(visitor.violations);
    }
    Ok(violations)
}

fn check_file_attributes(path: &Path, file: &File, violations: &mut Vec<Violation>) {
    for attr in &file.attrs {
        if !attr.path().is_ident("allow")
            && !attr.path().is_ident("expect")
            && !attr.path().is_ident("cfg_attr")
        {
            continue;
        }
        let Some(details) = attribute_details(attr) else {
            continue;
        };
        for lint in details.lints {
            violations.push(Violation {
                path: path.to_path_buf(),
                line: attr.path().span().start().line,
                message: format!("file-level {lint} exceptions are not permitted"),
            });
        }
    }
}

fn workspace_root() -> io::Result<PathBuf> {
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "CARGO_MANIFEST_DIR is not set"))?;
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "workspace root is not available"))
}

fn main() -> ExitCode {
    match workspace_root().and_then(|root| scan_sources(&root)) {
        Ok(violations) if violations.is_empty() => ExitCode::SUCCESS,
        Ok(mut violations) => {
            violations.sort_by(|left, right| {
                left.path
                    .cmp(&right.path)
                    .then(left.line.cmp(&right.line))
                    .then(left.message.cmp(&right.message))
            });
            for violation in violations {
                eprintln!(
                    "{}:{}: {}",
                    violation.path.display(),
                    violation.line,
                    violation.message
                );
            }
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("rust-style-check: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(source: &str) -> FunctionMetrics {
        let file = syn::parse_file(source).expect("fixture parses");
        let Item::Fn(function) = &file.items[0] else {
            panic!("fixture starts with a function");
        };
        let mut metrics = FunctionMetrics::default();
        metrics.visit_block(&function.block);
        metrics.max_nesting = metrics.max_nesting.saturating_sub(1);
        metrics
    }

    #[test]
    fn table_driven_nesting_fixtures() {
        for (source, expected) in [
            ("fn sample() { if true { while true {} } }", 2),
            (
                "fn sample() { match 1 { 1 => if true { {} }, _ => {} } }",
                2,
            ),
            (
                "fn sample() { for _ in 0..1 { loop { if true { while true {} } } } }",
                4,
            ),
        ] {
            assert_eq!(metrics(source).max_nesting, expected);
        }
    }

    #[test]
    fn table_driven_function_shape_fixtures() {
        for (source, expected) in [
            ("fn sample(a: u8, b: u8) {}", (2, 0)),
            (
                "fn sample(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8, g: u8, h: u8) {}",
                (8, 0),
            ),
        ] {
            let file = syn::parse_file(source).expect("fixture parses");
            let Item::Fn(function) = &file.items[0] else {
                panic!("fixture starts with a function");
            };
            let args = function
                .sig
                .inputs
                .iter()
                .filter(|arg| matches!(arg, FnArg::Typed(_)))
                .count();
            let mut metrics = FunctionMetrics::default();
            metrics.visit_block(&function.block);
            assert_eq!((args, metrics.max_nesting.saturating_sub(1)), expected);
        }
    }

    #[test]
    fn table_driven_clippy_line_count_fixtures() {
        for (source, expected) in [
            ("fn sample() {\n    // ignored\n    let value = 1;\n}\n", 1),
            (
                "fn sample() {\n    let value = 1; /* trailing comment */\n    value.to_string();\n}\n",
                2,
            ),
            ("fn sample() {\n    /* block\n       comment */\n}\n", 0),
        ] {
            let file = syn::parse_file(source).expect("fixture parses");
            let Item::Fn(function) = &file.items[0] else {
                panic!("fixture starts with a function");
            };
            assert_eq!(clippy_line_count(source, &function.block), expected);
        }
    }

    #[test]
    fn file_level_structural_attributes_are_rejected() {
        let file = syn::parse_file("#![allow(clippy::too_many_lines)]\nfn sample() {}")
            .expect("fixture parses");
        let path = Path::new("fixture.rs");
        let mut violations = Vec::new();
        check_file_attributes(path, &file, &mut violations);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("file-level"));
    }

    #[test]
    fn argument_exceptions_are_rejected_even_when_local() {
        let file = syn::parse_file(
            "#[allow(clippy::too_many_arguments, reason = \"fixture\")]\nfn sample() {}",
        )
        .expect("fixture parses");
        let Item::Fn(function) = &file.items[0] else {
            panic!("fixture starts with a function");
        };
        let mut visitor = SourceVisitor {
            path: Path::new("fixture.rs"),
            source: "",
            violations: Vec::new(),
        };
        visitor.visit_item_fn(function);
        assert!(
            visitor
                .violations
                .iter()
                .any(|violation| violation.message.contains("too_many_arguments"))
        );
    }

    #[test]
    fn structural_allow_exceptions_are_rejected() {
        let file = syn::parse_file(
            "#[allow(clippy::too_many_lines, reason = \"fixture\")]\nfn sample() {}",
        )
        .expect("fixture parses");
        let Item::Fn(function) = &file.items[0] else {
            panic!("fixture starts with a function");
        };
        let mut visitor = SourceVisitor {
            path: Path::new("fixture.rs"),
            source: "",
            violations: Vec::new(),
        };
        visitor.visit_item_fn(function);
        assert!(
            visitor
                .violations
                .iter()
                .any(|violation| violation.message.contains("must use #[expect]"))
        );
    }

    #[test]
    fn module_level_structural_attributes_are_rejected() {
        let file = syn::parse_file(
            "#[allow(clippy::excessive_nesting, reason = \"fixture\")]\nmod nested { fn sample() {} }",
        )
        .expect("fixture parses");
        let module = match &file.items[0] {
            Item::Mod(module) => module,
            _ => panic!("fixture starts with a module"),
        };
        let mut visitor = SourceVisitor {
            path: Path::new("fixture.rs"),
            source: "",
            violations: Vec::new(),
        };
        visitor.visit_item_mod(module);
        assert!(
            visitor
                .violations
                .iter()
                .any(|violation| violation.message.contains("container-level"))
        );
    }

    #[test]
    fn trait_level_structural_attributes_are_rejected() {
        let file = syn::parse_file(
            "#[expect(clippy::too_many_lines, reason = \"fixture\")]\ntrait Nested { fn sample(); }",
        )
        .expect("fixture parses");
        let Item::Trait(trait_item) = &file.items[0] else {
            panic!("fixture starts with a trait");
        };
        let mut visitor = SourceVisitor {
            path: Path::new("fixture.rs"),
            source: "",
            violations: Vec::new(),
        };
        visitor.visit_item_trait(trait_item);
        assert!(
            visitor
                .violations
                .iter()
                .any(|violation| violation.message.contains("container-level"))
        );
    }

    #[test]
    fn lint_names_in_reason_text_do_not_create_exceptions() {
        let attr: Attribute = syn::parse_quote! {
            #[expect(clippy::needless_return, reason = "mentions too_many_lines")]
        };
        assert!(!has_local_expectation(&[attr], "too_many_lines"));
    }

    #[test]
    fn cfg_attr_structural_attributes_are_rejected() {
        let file = syn::parse_file(
            "#[cfg_attr(test, expect(clippy::too_many_lines, reason = \"fixture\"))]\nfn sample() {}",
        )
        .expect("fixture parses");
        let Item::Fn(function) = &file.items[0] else {
            panic!("fixture starts with a function");
        };
        let mut visitor = SourceVisitor {
            path: Path::new("fixture.rs"),
            source: "",
            violations: Vec::new(),
        };
        visitor.visit_item_fn(function);
        assert!(
            visitor
                .violations
                .iter()
                .any(|violation| violation.message.contains("cfg_attr structural"))
        );
    }
}
