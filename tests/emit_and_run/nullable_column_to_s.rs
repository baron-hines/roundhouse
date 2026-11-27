//! `.to_s` on a nullable column read another record's column type by the bare name, and formatted
//! a Float with Go's `%g`: in `Comment`, `article.code` took Comment's own `*int64 code` and rendered
//! `%!d(string=hi)`, and Article's `score` of 1.0 rendered "1", where Ruby's Float#to_s is "1.0".

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("db/schema.rb", "    t.text \"body\"\n    t.datetime \"created_at\", null: false\n    t.datetime \"updated_at\", null: false\n  end\n\n  create_table \"comments\"",
            "    t.text \"body\"\n    t.string \"code\"\n    t.float \"score\"\n    t.boolean \"flag\"\n    t.boolean \"pinned\"\n    t.datetime \"created_at\", null: false\n    t.datetime \"updated_at\", null: false\n  end\n\n  create_table \"comments\"")
        .edit("db/schema.rb", "    t.string \"commenter\"\n", "    t.string \"commenter\"\n    t.integer \"code\"\n")
        .edit("app/models/comment.rb", "  belongs_to :article\n",
            "  belongs_to :article\n\n  def article_code\n    article.code.to_s\n  end\n\n  def article_score\n    article.score.to_s\n  end\n\n  def code_or_title\n    (article.code || article.title).to_s\n  end\n\n  def code_or_title_size\n    (article.code || article.title).length\n  end\n\n  def code_title_or_none\n    article.code || article.title || \"none\"\n  end\n\n  def flag_or_pinned\n    (article.flag || article.pinned).to_s\n  end\n\n  def flag_pinned_or_none\n    ((article.flag || article.pinned) || \"none\").to_s\n  end\n\n  def flag_or_false\n    (article.flag || false).to_s\n  end\n")
        .edit("test/models/comment_test.rb", "  test \"creates a comment on an article\" do\n",
            "  test \"renders another record's nullable columns\" do\n    comment = comments(:one)\n    assert_equal \"\", comment.article_code\n    assert_equal \"\", comment.article_score\n    assert_equal \"Getting Started with Rails\", comment.code_or_title\n    assert_equal 26, comment.code_or_title_size\n    assert_equal \"Getting Started with Rails\", comment.code_title_or_none\n    assert_equal \"\", comment.flag_or_pinned\n    assert_equal \"none\", comment.flag_pinned_or_none\n    assert_equal \"false\", comment.flag_or_false\n    comment.article.update(flag: false, pinned: false)\n    assert_equal \"none\", comment.flag_pinned_or_none\n    comment.article.update(flag: false, pinned: true)\n    assert_equal \"true\", comment.flag_or_pinned\n    assert_equal \"true\", comment.flag_pinned_or_none\n    comment.article.update(flag: true, pinned: false)\n    assert_equal \"true\", comment.flag_or_pinned\n    comment.update(code: 7)\n    comment.article.update(code: \"hi\", score: 1.0)\n    assert_equal \"hi\", comment.article_code\n    assert_equal \"hi\", comment.code_or_title\n    assert_equal 2, comment.code_or_title_size\n    assert_equal \"hi\", comment.code_title_or_none\n    assert_equal \"1.0\", comment.article_score\n    comment.article.update(score: 0.0)\n    assert_equal \"0.0\", comment.article_score\n    comment.article.update(score: 2.5)\n    assert_equal \"2.5\", comment.article_score\n    comment.article.update(score: 1.0e20)\n    assert_equal \"1.0e+20\", comment.article_score\n    comment.article.update(score: 0.00001)\n    assert_equal \"1.0e-05\", comment.article_score\n  end\n\n  test \"creates a comment on an article\" do\n")
}

#[test]
fn another_record_s_nullable_columns_render_as_ruby_s() {
    app().run_test("test/models/comment_test.rb").assert_passes();
}

#[test]
#[ignore = "requires Go; builds and runs an emitted app"]
fn another_record_s_nullable_columns_render_as_ruby_s_on_go() {
    let (emitted, errors) = app().emit(roundhouse::project::BuildTarget::Go);
    assert!(errors.is_empty(), "{errors:?}");
    super::run_emitted(&emitted, &[("go", &["mod", "tidy"]), ("go", &["test", "./..."])]);
}
