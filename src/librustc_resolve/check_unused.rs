// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


//
// Unused import checking
//
// Although this is mostly a lint pass, it lives in here because it depends on
// resolve data structures and because it finalises the privacy information for
// `use` directives.
//
// Unused trait imports can't be checked until the method resolution. We save
// candidates here, and do the actual check in librustc_typeck/check_unused.rs.

use std::ops::{Deref, DerefMut};

use Resolver;
use resolve_imports::ImportDirectiveSubclass;

use rustc::{lint, ty};
use rustc::util::nodemap::NodeMap;
use syntax::ast;
use syntax::codemap::CodeMap;
use syntax::visit::{self, Visitor};
use syntax_pos::{Span, MultiSpan, DUMMY_SP, SpanSnippetError};


struct UnusedImportCheckVisitor<'a, 'b: 'a> {
    resolver: &'a mut Resolver<'b>,
    /// All the (so far) unused imports, grouped path list
    unused_imports: NodeMap<NodeMap<Span>>,
    base_id: ast::NodeId,
    item_span: Span,
    trees: NodeMap<&'a ast::UseTree>,
    item_spans: NodeMap<Span>,
}

// Deref and DerefMut impls allow treating UnusedImportCheckVisitor as Resolver.
impl<'a, 'b> Deref for UnusedImportCheckVisitor<'a, 'b> {
    type Target = Resolver<'b>;

    fn deref<'c>(&'c self) -> &'c Resolver<'b> {
        &*self.resolver
    }
}

impl<'a, 'b> DerefMut for UnusedImportCheckVisitor<'a, 'b> {
    fn deref_mut<'c>(&'c mut self) -> &'c mut Resolver<'b> {
        &mut *self.resolver
    }
}

impl<'a, 'b> UnusedImportCheckVisitor<'a, 'b> {
    // We have information about whether `use` (import) directives are actually
    // used now. If an import is not used at all, we signal a lint error.
    fn check_import(&mut self, item_id: ast::NodeId, id: ast::NodeId, span: Span) {
        let mut used = false;
        self.per_ns(|this, ns| used |= this.used_imports.contains(&(id, ns)));
        if !used {
            if self.maybe_unused_trait_imports.contains(&id) {
                // Check later.
                return;
            }
            self.unused_imports.entry(item_id).or_insert_with(NodeMap).insert(id, span);
        } else {
            // This trait import is definitely used, in a way other than
            // method resolution.
            self.maybe_unused_trait_imports.remove(&id);
            if let Some(i) = self.unused_imports.get_mut(&item_id) {
                i.remove(&id);
            }
        }
    }
}

impl<'a, 'b> Visitor<'a> for UnusedImportCheckVisitor<'a, 'b> {
    fn visit_item(&mut self, item: &'a ast::Item) {
        self.item_span = item.span;

        // Ignore is_public import statements because there's no way to be sure
        // whether they're used or not. Also ignore imports with a dummy span
        // because this means that they were generated in some fashion by the
        // compiler and we don't need to consider them.
        if let ast::ItemKind::Use(..) = item.node {
            if item.vis.node == ast::VisibilityKind::Public || item.span.source_equal(&DUMMY_SP) {
                return;
            }
        }

        visit::walk_item(self, item);
    }

    fn visit_use_tree(&mut self, use_tree: &'a ast::UseTree, id: ast::NodeId, nested: bool) {
        // Use the base UseTree's NodeId as the item id
        // This allows the grouping of all the lints in the same item
        if !nested {
            self.base_id = id;
            self.trees.insert(id, &use_tree);
            self.item_spans.insert(id, self.item_span);
        }

        if let ast::UseTreeKind::Nested(ref items) = use_tree.kind {
            // If it's the parent group, cover the entire use item
            let span = if nested {
                use_tree.span
            } else {
                self.item_span
            };

            if items.len() == 0 {
                self.unused_imports
                    .entry(self.base_id)
                    .or_insert_with(NodeMap)
                    .insert(id, span);
            }
        } else {
            let base_id = self.base_id;
            self.check_import(base_id, id, use_tree.span);
        }

        visit::walk_use_tree(self, use_tree, id);
    }
}

fn suggest_snip(tree_id: (&ast::UseTree, ast::NodeId), unused: &NodeMap<Span>, codemap: &CodeMap)
               -> Result<Option<String>, SpanSnippetError> {

    let (tree, id) = tree_id;

    Ok(if unused.contains_key(&id) {
        None
    } else if let ast::UseTreeKind::Nested(ref items) = tree.kind {
        let id_to_snip = items.iter().map(
            |&(ref tree, id)| Ok((id, suggest_snip((&tree, id), unused, codemap)?))
        ).collect::<Result<NodeMap<_>, _>>()?;

        match id_to_snip.values().filter(|opt| opt.is_some()).count() {

            0 => None,
            // Collapse {x} into x, if we caused there to be only a single item
            1 if items.len() > 1 => Some(codemap.span_to_snippet(tree.prefix.span)?
                                         + "::"
                                         + &id_to_snip.values()
                                             .filter_map(|x| x.clone()).next().unwrap()
                                    ),
            _ => {
                let mut added_any = false;
                let mut str = codemap.span_to_snippet(tree.span.until(items[0].0.span))?;

                if let Some(ref snip) = id_to_snip[&items[0].1] {
                    str += &snip;
                    added_any = true;
                }
                for window in items.windows(2) {
                    let (ref prev, _) = window[0];
                    let (ref this, this_id) = window[1];
                    if let Some(ref snip) = id_to_snip[&this_id] {
                        if added_any {
                            str += &codemap.span_to_snippet(prev.span.between(this.span))?;
                        }
                        str += &snip;
                        added_any = true;
                    }
                }

                // }, and any tailing comma or spacing before it
                str += &codemap.span_to_snippet(
                    tree.span.trim_start(items.last().unwrap().0.span).unwrap()
                )?;

                Some(str)
            }
        }
    } else {
        Some(codemap.span_to_snippet(tree.span)?)
    })
}

pub fn check_crate(resolver: &mut Resolver, krate: &ast::Crate) {
    for directive in resolver.potentially_unused_imports.iter() {
        match directive.subclass {
            _ if directive.used.get() ||
                 directive.vis.get() == ty::Visibility::Public ||
                 directive.span.source_equal(&DUMMY_SP) => {}
            ImportDirectiveSubclass::ExternCrate(_) => {
                resolver.maybe_unused_extern_crates.push((directive.id, directive.span));
            }
            ImportDirectiveSubclass::MacroUse => {
                let lint = lint::builtin::UNUSED_IMPORTS;
                let msg = "unused `#[macro_use]` import";
                resolver.session.buffer_lint(lint, directive.id, directive.span, msg);
            }
            _ => {}
        }
    }

    let mut visitor = UnusedImportCheckVisitor {
        resolver,
        unused_imports: NodeMap(),
        base_id: ast::DUMMY_NODE_ID,
        item_span: DUMMY_SP,
        trees: NodeMap(),
        item_spans: NodeMap(),
    };
    visit::walk_crate(&mut visitor, krate);

    for (id, spans_map) in &visitor.unused_imports {
        let len = spans_map.len();
        let mut spans = spans_map.values().map(|s| *s).collect::<Vec<Span>>();
        spans.sort();
        let ms = MultiSpan::from_spans(spans.clone());
        let mut span_snippets = spans.iter()
            .filter_map(|s| {
                match visitor.session.codemap().span_to_snippet(*s) {
                    Ok(s) => Some(format!("`{}`", s)),
                    _ => None,
                }
            }).collect::<Vec<String>>();
        span_snippets.sort();
        let msg = format!("unused import{}{}",
                          if len > 1 { "s" } else { "" },
                          if span_snippets.len() > 0 {
                              format!(": {}", span_snippets.join(", "))
                          } else {
                              String::new()
                          });

        let item_span = visitor.item_spans[&id];
        let suggestion_snip = suggest_snip(
            (&visitor.trees[id], *id),
            &spans_map,
            &visitor.session.codemap()
        ).unwrap();
        let diag = if let Some(tree_snip) = suggestion_snip {
            lint::builtin::BuiltinLintDiagnostics::PartiallyUnusedImport(
                visitor.session.codemap().span_to_snippet(
                    item_span.until(visitor.trees[&id].span)
                ).unwrap() + &tree_snip,
                item_span
            )
        } else {
            lint::builtin::BuiltinLintDiagnostics::UnusedImport(item_span)
        };

        visitor.session.buffer_lint_with_diagnostic(
            lint::builtin::UNUSED_IMPORTS, *id, ms, &msg, diag
        );
    }
}
