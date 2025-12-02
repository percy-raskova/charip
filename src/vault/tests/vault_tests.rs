use std::path::Path;

use itertools::Itertools;
use tower_lsp::lsp_types::{Position, Range};

use crate::vault::{HeadingLevel, ReferenceData};
use crate::vault::{MDLinkReferenceDefinition, Refname};

use crate::vault::Reference::Footnote;
use crate::vault::{
    MDFile, MDFootnote, MDHeading, MDIndexedBlock, MDTag, Reference, Referenceable,
};

#[test]
fn md_link_parsing() {
    let text = "Test text test text [link](path/to/link)";

    let parsed = Reference::new(text, "test.md").collect_vec();

    let expected = vec![Reference::MDFileLink(ReferenceData {
        reference_text: "path/to/link".into(),
        display_text: Some("link".into()),
        range: Range {
            start: Position {
                line: 0,
                character: 20,
            },
            end: Position {
                line: 0,
                character: 40,
            },
        }
        .into(),
    })];

    assert_eq!(parsed, expected);

    let text = "Test text test text [link](./path/to/link)";

    let parsed = Reference::new(text, "test.md").collect_vec();

    let expected = vec![Reference::MDFileLink(ReferenceData {
        reference_text: "./path/to/link".into(),
        display_text: Some("link".into()),
        range: Range {
            start: Position {
                line: 0,
                character: 20,
            },
            end: Position {
                line: 0,
                character: 42,
            },
        }
        .into(),
    })];

    assert_eq!(parsed, expected);

    let text = "Test text test text [link](./path/to/link.md)";

    let parsed = Reference::new(text, "test.md").collect_vec();

    let expected = vec![Reference::MDFileLink(ReferenceData {
        reference_text: "./path/to/link".into(),
        display_text: Some("link".into()),
        range: Range {
            start: Position {
                line: 0,
                character: 20,
            },
            end: Position {
                line: 0,
                character: 45,
            },
        }
        .into(),
    })];

    assert_eq!(parsed, expected)
}

#[test]
fn advanced_md_link_parsing() {
    let text = "Test text test text [link](<path to/link>)";

    let parsed = Reference::new(text, "test.md").collect_vec();

    let expected = vec![Reference::MDFileLink(ReferenceData {
        reference_text: "path to/link".into(),
        display_text: Some("link".into()),
        range: Range {
            start: Position {
                line: 0,
                character: 20,
            },
            end: Position {
                line: 0,
                character: 42,
            },
        }
        .into(),
    })];

    assert_eq!(parsed, expected);

    let text = "Test text test text [link](<path/to/link.md#heading>)";

    let parsed = Reference::new(text, "test.md").collect_vec();

    let expected = vec![Reference::MDHeadingLink(
        ReferenceData {
            reference_text: "path/to/link#heading".into(),
            display_text: Some("link".into()),
            range: Range {
                start: Position {
                    line: 0,
                    character: 20,
                },
                end: Position {
                    line: 0,
                    character: 53,
                },
            }
            .into(),
        },
        "path/to/link".into(),
        "heading".into(),
    )];

    assert_eq!(parsed, expected)
}

#[test]
fn md_heading_link_parsing() {
    let text = "Test text test text [link](path/to/link#heading)";

    let parsed = Reference::new(text, "test.md").collect_vec();

    let expected = vec![Reference::MDHeadingLink(
        ReferenceData {
            reference_text: "path/to/link#heading".into(),
            display_text: Some("link".into()),
            range: Range {
                start: Position {
                    line: 0,
                    character: 20,
                },
                end: Position {
                    line: 0,
                    character: 48,
                },
            }
            .into(),
        },
        "path/to/link".into(),
        "heading".into(),
    )];

    assert_eq!(parsed, expected);

    let text = "Test text test text [link](path/to/link.md#heading)";

    let parsed = Reference::new(text, "test.md").collect_vec();

    let expected = vec![Reference::MDHeadingLink(
        ReferenceData {
            reference_text: "path/to/link#heading".into(),
            display_text: Some("link".into()),
            range: Range {
                start: Position {
                    line: 0,
                    character: 20,
                },
                end: Position {
                    line: 0,
                    character: 51,
                },
            }
            .into(),
        },
        "path/to/link".into(),
        "heading".into(),
    )];

    assert_eq!(parsed, expected)
}

#[test]
fn md_block_link_parsing() {
    let text = "Test text test text [link](path/to/link#^index1)";

    let parsed = Reference::new(text, "test.md").collect_vec();

    let expected = vec![Reference::MDIndexedBlockLink(
        ReferenceData {
            reference_text: "path/to/link#^index1".into(),
            display_text: Some("link".into()),
            range: Range {
                start: Position {
                    line: 0,
                    character: 20,
                },
                end: Position {
                    line: 0,
                    character: 48,
                },
            }
            .into(),
        },
        "path/to/link".into(),
        "index1".into(),
    )];

    assert_eq!(parsed, expected);

    let text = "Test text test text [link](path/to/link.md#^index1)";

    let parsed = Reference::new(text, "test.md").collect_vec();

    let expected = vec![Reference::MDIndexedBlockLink(
        ReferenceData {
            reference_text: "path/to/link#^index1".into(),
            display_text: Some("link".into()),
            range: Range {
                start: Position {
                    line: 0,
                    character: 20,
                },
                end: Position {
                    line: 0,
                    character: 51,
                },
            }
            .into(),
        },
        "path/to/link".into(),
        "index1".into(),
    )];

    assert_eq!(parsed, expected)
}

#[test]
fn md_link_with_trailing_parentheses_parsing() {
    // [Issue 260](https://github.com/Feel-ix-343/markdown-oxide/issues/260) covers an issue with parentheses on a new line after a mdlink being parsed as another link.
    // Note: Text must not be indented 4+ spaces or it becomes a code block in markdown.

    let text = r#"
Buggy cross [link](path/to/link#^index1):

(this causes bug)
"#;

    let parsed = Reference::new(text, "test.md").collect_vec();

    let expected = vec![Reference::MDIndexedBlockLink(
        ReferenceData {
            reference_text: "path/to/link#^index1".into(),
            display_text: Some("link".into()),
            range: Range {
                start: Position {
                    line: 1,
                    character: 12,
                },
                end: Position {
                    line: 1,
                    character: 40,
                },
            }
            .into(),
        },
        "path/to/link".into(),
        "index1".into(),
    )];

    assert_eq!(parsed, expected);
}

#[test]
fn footnote_link_parsing() {
    let text = "This is a footnote[^1]

[^1]: This is not";
    let parsed = Reference::new(text, "test.md").collect_vec();
    let expected = vec![Footnote(ReferenceData {
        reference_text: "^1".into(),
        range: tower_lsp::lsp_types::Range {
            start: tower_lsp::lsp_types::Position {
                line: 0,
                character: 18,
            },
            end: tower_lsp::lsp_types::Position {
                line: 0,
                character: 22,
            },
        }
        .into(),
        ..ReferenceData::default()
    })];

    assert_eq!(parsed, expected)
}

#[test]
fn multi_footnote_link_parsing() {
    let text = "This is a footnote[^1][^2][^3]

[^1]: This is not
[^2]: This is not
[^3]: This is not";
    let parsed = Reference::new(text, "test.md").collect_vec();
    let expected = vec![
        Footnote(ReferenceData {
            reference_text: "^1".into(),
            range: tower_lsp::lsp_types::Range {
                start: tower_lsp::lsp_types::Position {
                    line: 0,
                    character: 18,
                },
                end: tower_lsp::lsp_types::Position {
                    line: 0,
                    character: 22,
                },
            }
            .into(),
            ..ReferenceData::default()
        }),
        Footnote(ReferenceData {
            reference_text: "^2".into(),
            range: tower_lsp::lsp_types::Range {
                start: tower_lsp::lsp_types::Position {
                    line: 0,
                    character: 22,
                },
                end: tower_lsp::lsp_types::Position {
                    line: 0,
                    character: 26,
                },
            }
            .into(),
            ..ReferenceData::default()
        }),
        Footnote(ReferenceData {
            reference_text: "^3".into(),
            range: tower_lsp::lsp_types::Range {
                start: tower_lsp::lsp_types::Position {
                    line: 0,
                    character: 26,
                },
                end: tower_lsp::lsp_types::Position {
                    line: 0,
                    character: 30,
                },
            }
            .into(),
            ..ReferenceData::default()
        }),
    ];

    assert_eq!(parsed, expected)
}

#[test]
fn heading_parsing() {
    let text = r"# This is a heading

Some more text on the second line

Some text under it

some mroe text

more text


## This shoudl be a heading!";

    let parsed: Vec<_> = MDHeading::new(text).collect();

    let expected = vec![
        MDHeading {
            heading_text: "This is a heading".into(),
            range: tower_lsp::lsp_types::Range {
                start: tower_lsp::lsp_types::Position {
                    line: 0,
                    character: 0,
                },
                end: tower_lsp::lsp_types::Position {
                    line: 0,
                    character: 19,
                },
            }
            .into(),
            ..Default::default()
        },
        MDHeading {
            heading_text: "This shoudl be a heading!".into(),
            range: tower_lsp::lsp_types::Range {
                start: tower_lsp::lsp_types::Position {
                    line: 11,
                    character: 0,
                },
                end: tower_lsp::lsp_types::Position {
                    line: 11,
                    character: 28,
                },
            }
            .into(),
            level: HeadingLevel(2),
        },
    ];

    assert_eq!(parsed, expected)
}

#[test]
fn indexed_block_parsing() {
    let text = r"# This is a heading

        Some more text on the second line fjasdkl fdkaslfjdaskl jfklas fjkldasj fkldsajfkld
        fasd fjkldasfjkldasfj kldasfj dklas
        afd asjklfdjasklfj dklasfjkdlasjfkldjasklfasd
        af djaskl
        f jdaskfjdklasfj kldsafjkldsa
        f jasdkfj dsaklfdsal ^12345

        Some text under it
        some mroe text
        more text";

    let parsed = MDIndexedBlock::new(text).collect_vec();

    assert_eq!(parsed[0].index, "12345")
}

#[test]
fn test_linkable_reference() {
    use crate::vault::graph::DocumentNode;

    let path = Path::new("/home/vault/test.md");
    let path_buf = path.to_path_buf();
    let doc_node = DocumentNode {
        path: path_buf.clone(),
        ..Default::default()
    };
    let linkable: Referenceable = Referenceable::File(&path_buf, &doc_node);

    let root_dir = Path::new("/home/vault");
    let refname = linkable.get_refname(root_dir);

    assert_eq!(
        refname,
        Some(Refname {
            full_refname: "test".into(),
            path: "test".to_string().into(),
            ..Default::default()
        })
    )
}

#[test]
fn test_linkable_reference_heading() {
    let path = Path::new("/home/vault/test.md");
    let path_buf = path.to_path_buf();
    let md_heading = MDHeading {
        heading_text: "Test Heading".into(),
        range: tower_lsp::lsp_types::Range::default().into(),
        ..Default::default()
    };
    let linkable: Referenceable = Referenceable::Heading(&path_buf, &md_heading);

    let root_dir = Path::new("/home/vault");
    let refname = linkable.get_refname(root_dir);

    assert_eq!(
        refname,
        Some(Refname {
            full_refname: "test#Test Heading".to_string(),
            path: Some("test".to_string()),
            infile_ref: Some("Test Heading".to_string())
        })
    )
}

#[test]
fn test_linkable_reference_indexed_block() {
    let path = Path::new("/home/vault/test.md");
    let path_buf = path.to_path_buf();
    let md_indexed_block = MDIndexedBlock {
        index: "12345".into(),
        range: tower_lsp::lsp_types::Range::default().into(),
    };
    let linkable: Referenceable = Referenceable::IndexedBlock(&path_buf, &md_indexed_block);

    let root_dir = Path::new("/home/vault");
    let refname = linkable.get_refname(root_dir);

    assert_eq!(
        refname,
        Some(Refname {
            full_refname: "test#^12345".into(),
            path: Some("test".into()),
            infile_ref: "^12345".to_string().into()
        })
    )
}

#[test]
fn test_comprehensive_tag_parsing() {
    let text = r##"# This is a heading

This is a #tag and another #tag/subtag

Chinese: #中文标签 #中文/子标签
Japanese: #テストtag #タグ/子タグ
Korean: #한국어 #한글/서브태그

Edge cases:
- Not a tag: word#notag [[link#not a tag]]
- Number start: #7invalid
- Special chars: #-/_/tag
- In quotes: "Text #引用中的标签 text"
- Complex: #MapOfContext/apworld

#正常标签
    "##;
    let expected: Vec<MDTag> = vec![
        MDTag {
            tag_ref: "tag".into(),
            range: Range {
                start: Position {
                    line: 2,
                    character: 10,
                },
                end: Position {
                    line: 2,
                    character: 14,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "tag/subtag".into(),
            range: Range {
                start: Position {
                    line: 2,
                    character: 27,
                },
                end: Position {
                    line: 2,
                    character: 38,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "中文标签".into(),
            range: Range {
                start: Position {
                    line: 4,
                    character: 9,
                },
                end: Position {
                    line: 4,
                    character: 14,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "中文/子标签".into(),
            range: Range {
                start: Position {
                    line: 4,
                    character: 15,
                },
                end: Position {
                    line: 4,
                    character: 22,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "テストtag".into(),
            range: Range {
                start: Position {
                    line: 5,
                    character: 10,
                },
                end: Position {
                    line: 5,
                    character: 17,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "タグ/子タグ".into(),
            range: Range {
                start: Position {
                    line: 5,
                    character: 18,
                },
                end: Position {
                    line: 5,
                    character: 25,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "한국어".into(),
            range: Range {
                start: Position {
                    line: 6,
                    character: 8,
                },
                end: Position {
                    line: 6,
                    character: 12,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "한글/서브태그".into(),
            range: Range {
                start: Position {
                    line: 6,
                    character: 13,
                },
                end: Position {
                    line: 6,
                    character: 21,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "-/_/tag".into(),
            range: Range {
                start: Position {
                    line: 11,
                    character: 17,
                },
                end: Position {
                    line: 11,
                    character: 25,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "引用中的标签".into(),
            range: Range {
                start: Position {
                    line: 12,
                    character: 19,
                },
                end: Position {
                    line: 12,
                    character: 26,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "MapOfContext/apworld".into(),
            range: Range {
                start: Position {
                    line: 13,
                    character: 11,
                },
                end: Position {
                    line: 13,
                    character: 32,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "正常标签".into(),
            range: Range {
                start: Position {
                    line: 15,
                    character: 0,
                },
                end: Position {
                    line: 15,
                    character: 5,
                },
            }
            .into(),
        },
    ];

    let parsed = MDTag::new(text).collect_vec();
    assert_eq!(parsed, expected);
}

#[test]
fn test_all_quote_types_in_tags() {
    // Test tags with all types of quotes (Chinese and English, single and double)
    let text = r##"
Chinese double quotes: #测试"引号"标签
English double quotes: #test"quotes"tag
English single quotes: #test'quotes'tag
Curly single quotes: #test'quotes'tag
Mixed quotes: #混合"quotes'测试'标签"
Plain tag: #plaintext
    "##;
    let expected: Vec<MDTag> = vec![
        MDTag {
            tag_ref: "测试\"引号\"标签".into(),
            range: Range {
                start: Position {
                    line: 1,
                    character: 23,
                },
                end: Position {
                    line: 1,
                    character: 32,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "test\"quotes\"tag".into(),
            range: Range {
                start: Position {
                    line: 2,
                    character: 23,
                },
                end: Position {
                    line: 2,
                    character: 39,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "test'quotes'tag".into(),
            range: Range {
                start: Position {
                    line: 3,
                    character: 23,
                },
                end: Position {
                    line: 3,
                    character: 39,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "test'quotes'tag".into(),
            range: Range {
                start: Position {
                    line: 4,
                    character: 21,
                },
                end: Position {
                    line: 4,
                    character: 37,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "混合\"quotes'测试'标签\"".into(),
            range: Range {
                start: Position {
                    line: 5,
                    character: 14,
                },
                end: Position {
                    line: 5,
                    character: 31,
                },
            }
            .into(),
        },
        MDTag {
            tag_ref: "plaintext".into(),
            range: Range {
                start: Position {
                    line: 6,
                    character: 11,
                },
                end: Position {
                    line: 6,
                    character: 21,
                },
            }
            .into(),
        },
    ];
    let parsed = MDTag::new(text).collect_vec();

    assert_eq!(parsed, expected);
}

#[test]
fn test_footnote() {
    let text = "[^1]: This is a footnote";
    let parsed = MDFootnote::new(text).collect_vec();
    let expected = vec![MDFootnote {
        index: "^1".into(),
        footnote_text: "This is a footnote".into(),
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 24,
            },
        }
        .into(),
    }];

    assert_eq!(parsed, expected);

    let text = r"# This is a heading

Referenced[^1]

[^1]: Footnote here

Continued

[^2]: Another footnote
[^a]:Third footnot3
";
    let parsed = MDFootnote::new(text).collect_vec();
    let expected = vec![
        MDFootnote {
            index: "^1".into(),
            footnote_text: "Footnote here".into(),
            range: Range {
                start: Position {
                    line: 4,
                    character: 0,
                },
                end: Position {
                    line: 4,
                    character: 19,
                },
            }
            .into(),
        },
        MDFootnote {
            index: "^2".into(),
            footnote_text: "Another footnote".into(),
            range: Range {
                start: Position {
                    line: 8,
                    character: 0,
                },
                end: Position {
                    line: 8,
                    character: 22,
                },
            }
            .into(),
        },
        MDFootnote {
            index: "^a".into(),
            footnote_text: "Third footnot3".into(),
            range: Range {
                start: Position {
                    line: 9,
                    character: 0,
                },
                end: Position {
                    line: 9,
                    character: 19,
                },
            }
            .into(),
        },
    ];

    assert_eq!(parsed, expected)
}

#[test]
fn parse_link_ref_def() {
    let text = "[ab]: ohreally";

    let parsed = MDLinkReferenceDefinition::new(text).collect_vec();

    let expected = vec![MDLinkReferenceDefinition {
        link_ref_name: "ab".into(),
        url: "ohreally".into(),
        title: None,
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 14,
            },
        }
        .into(),
    }];

    assert_eq!(parsed, expected);
}

#[test]
fn parse_link_ref() {
    let text = "This is a [link]j\n\n[link]: linktext";

    let parsed = Reference::new(text, "test.md").collect_vec();

    let expected = vec![Reference::LinkRef(ReferenceData {
        reference_text: "link".into(),
        range: Range {
            start: Position {
                line: 0,
                character: 10,
            },
            end: Position {
                line: 0,
                character: 16,
            },
        }
        .into(),
        ..ReferenceData::default()
    })];

    assert_eq!(parsed, expected);
}
#[test]
fn parse_url_encoded_link() {
    let text = " [f](file%20with%20spaces)";

    let parsed = Reference::new(text, "test.md").collect_vec();

    let expected = vec![Reference::MDFileLink(ReferenceData {
        reference_text: "file with spaces".into(),
        display_text: Some("f".into()),
        range: Range {
            start: Position {
                line: 0,
                character: 1,
            },
            end: Position {
                line: 0,
                character: 26,
            },
        }
        .into(),
    })];

    assert_eq!(parsed, expected);
}
#[test]
fn parse_weird_url_encoded_file_link() {
    // URL-encoded filename with unicode and special characters.
    // Note: The decoded URL contains `#` which is interpreted as a fragment separator.
    // This is correct URL behavior - if you want a literal `#` in the path, use `%2523`.
    let text = "[f](%D1%84%D0%B0%D0%B9%D0%BB%20with%20%C3%A9mojis%20%F0%9F%9A%80%20%26%20symbols%20%21%23%40%24%25%26%2A%28%29%2B%3D%7B%7D%7C%5C%22%5C%5C%3A%3B%3F)";

    let parsed = Reference::new(text, "test.md").collect_vec();

    // The `#` in the decoded URL creates a heading link
    // Path: "файл with émojis 🚀 & symbols !"
    // Fragment: "@$%&*()+={}|\"\\:;?"
    let expected = vec![Reference::MDHeadingLink(
        ReferenceData {
            reference_text: r##"файл with émojis 🚀 & symbols !#@$%&*()+={}|\"\\:;?"##.into(),
            display_text: Some("f".into()),
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 147,
                },
            }
            .into(),
        },
        r##"файл with émojis 🚀 & symbols !"##.into(),
        r##"@$%&*()+={}|\"\\:;?"##.into(),
    )];

    assert_eq!(parsed, expected);
}
