//! Ruby 4.0.7's public instance methods of the core containers, `Object`'s and `Kernel`'s among them, as
//! `Class#public_instance_methods` lists them: what an instance of a subclass answers without `method_missing`.

pub(crate) const HASH: &[&str] = &[
    "!", "!=", "!~", "<", "<=", "<=>", "==", "===",
    ">", ">=", "[]", "[]=", "__id__", "__send__", "all?", "any?",
    "assoc", "chain", "chunk", "chunk_while", "class", "clear", "clone", "collect",
    "collect_concat", "compact", "compact!", "compare_by_identity", "compare_by_identity?", "count", "cycle", "deconstruct_keys",
    "default", "default=", "default_proc", "default_proc=", "define_singleton_method", "delete", "delete_if", "detect",
    "dig", "display", "drop", "drop_while", "dup", "each", "each_cons", "each_entry",
    "each_key", "each_pair", "each_slice", "each_value", "each_with_index", "each_with_object", "empty?", "entries",
    "enum_for", "eql?", "equal?", "except", "extend", "fetch", "fetch_values", "filter",
    "filter!", "filter_map", "find", "find_all", "find_index", "first", "flat_map", "flatten",
    "freeze", "frozen?", "grep", "grep_v", "group_by", "has_key?", "has_value?", "hash",
    "include?", "inject", "inspect", "instance_eval", "instance_exec", "instance_of?", "instance_variable_defined?", "instance_variable_get",
    "instance_variable_set", "instance_variables", "invert", "is_a?", "itself", "keep_if", "key", "key?",
    "keys", "kind_of?", "lazy", "length", "map", "max", "max_by", "member?",
    "merge", "merge!", "method", "methods", "min", "min_by", "minmax", "minmax_by",
    "nil?", "none?", "object_id", "one?", "partition", "private_methods", "protected_methods", "public_method",
    "public_methods", "public_send", "rassoc", "reduce", "rehash", "reject", "reject!", "remove_instance_variable",
    "replace", "respond_to?", "reverse_each", "select", "select!", "send", "shift", "singleton_class",
    "singleton_method", "singleton_methods", "size", "slice", "slice_after", "slice_before", "slice_when", "sort",
    "sort_by", "store", "sum", "take", "take_while", "tally", "tap", "then",
    "to_a", "to_enum", "to_h", "to_hash", "to_proc", "to_s", "to_set", "transform_keys",
    "transform_keys!", "transform_values", "transform_values!", "uniq", "update", "value?", "values", "values_at",
    "yield_self", "zip",
];

pub(crate) const ARRAY: &[&str] = &[
    "!", "!=", "!~", "&", "*", "+", "-", "<<",
    "<=>", "==", "===", "[]", "[]=", "__id__", "__send__", "all?",
    "any?", "append", "assoc", "at", "bsearch", "bsearch_index", "chain", "chunk",
    "chunk_while", "class", "clear", "clone", "collect", "collect!", "collect_concat", "combination",
    "compact", "compact!", "concat", "count", "cycle", "deconstruct", "define_singleton_method", "delete",
    "delete_at", "delete_if", "detect", "difference", "dig", "display", "drop", "drop_while",
    "dup", "each", "each_cons", "each_entry", "each_index", "each_slice", "each_with_index", "each_with_object",
    "empty?", "entries", "enum_for", "eql?", "equal?", "extend", "fetch", "fetch_values",
    "fill", "filter", "filter!", "filter_map", "find", "find_all", "find_index", "first",
    "flat_map", "flatten", "flatten!", "freeze", "frozen?", "grep", "grep_v", "group_by",
    "hash", "include?", "index", "inject", "insert", "inspect", "instance_eval", "instance_exec",
    "instance_of?", "instance_variable_defined?", "instance_variable_get", "instance_variable_set", "instance_variables", "intersect?", "intersection", "is_a?",
    "itself", "join", "keep_if", "kind_of?", "last", "lazy", "length", "map",
    "map!", "max", "max_by", "member?", "method", "methods", "min", "min_by",
    "minmax", "minmax_by", "nil?", "none?", "object_id", "one?", "pack", "partition",
    "permutation", "pop", "prepend", "private_methods", "product", "protected_methods", "public_method", "public_methods",
    "public_send", "push", "rassoc", "reduce", "reject", "reject!", "remove_instance_variable", "repeated_combination",
    "repeated_permutation", "replace", "respond_to?", "reverse", "reverse!", "reverse_each", "rfind", "rindex",
    "rotate", "rotate!", "sample", "select", "select!", "send", "shift", "shuffle",
    "shuffle!", "singleton_class", "singleton_method", "singleton_methods", "size", "slice", "slice!", "slice_after",
    "slice_before", "slice_when", "sort", "sort!", "sort_by", "sort_by!", "sum", "take",
    "take_while", "tally", "tap", "then", "to_a", "to_ary", "to_enum", "to_h",
    "to_s", "to_set", "transpose", "union", "uniq", "uniq!", "unshift", "values_at",
    "yield_self", "zip", "|",
];

pub(crate) const STRING: &[&str] = &[
    "!", "!=", "!~", "%", "*", "+", "+@", "-@",
    "<", "<<", "<=", "<=>", "==", "===", "=~", ">",
    ">=", "[]", "[]=", "__id__", "__send__", "append_as_bytes", "ascii_only?", "b",
    "between?", "byteindex", "byterindex", "bytes", "bytesize", "byteslice", "bytesplice", "capitalize",
    "capitalize!", "casecmp", "casecmp?", "center", "chars", "chomp", "chomp!", "chop",
    "chop!", "chr", "clamp", "class", "clear", "clone", "codepoints", "concat",
    "count", "crypt", "dedup", "define_singleton_method", "delete", "delete!", "delete_prefix", "delete_prefix!",
    "delete_suffix", "delete_suffix!", "display", "downcase", "downcase!", "dump", "dup", "each_byte",
    "each_char", "each_codepoint", "each_grapheme_cluster", "each_line", "empty?", "encode", "encode!", "encoding",
    "end_with?", "enum_for", "eql?", "equal?", "extend", "force_encoding", "freeze", "frozen?",
    "getbyte", "grapheme_clusters", "gsub", "gsub!", "hash", "hex", "include?", "index",
    "insert", "inspect", "instance_eval", "instance_exec", "instance_of?", "instance_variable_defined?", "instance_variable_get", "instance_variable_set",
    "instance_variables", "intern", "is_a?", "itself", "kind_of?", "length", "lines", "ljust",
    "lstrip", "lstrip!", "match", "match?", "method", "methods", "next", "next!",
    "nil?", "object_id", "oct", "ord", "partition", "prepend", "private_methods", "protected_methods",
    "public_method", "public_methods", "public_send", "remove_instance_variable", "replace", "respond_to?", "reverse", "reverse!",
    "rindex", "rjust", "rpartition", "rstrip", "rstrip!", "scan", "scrub", "scrub!",
    "send", "setbyte", "singleton_class", "singleton_method", "singleton_methods", "size", "slice", "slice!",
    "split", "squeeze", "squeeze!", "start_with?", "strip", "strip!", "sub", "sub!",
    "succ", "succ!", "sum", "swapcase", "swapcase!", "tap", "then", "to_c",
    "to_enum", "to_f", "to_i", "to_r", "to_s", "to_str", "to_sym", "tr",
    "tr!", "tr_s", "tr_s!", "undump", "unicode_normalize", "unicode_normalize!", "unicode_normalized?", "unpack",
    "unpack1", "upcase", "upcase!", "upto", "valid_encoding?", "yield_self",
];

pub(crate) const OBJECT: &[&str] = &[
    "!", "!=", "!~", "<=>", "==", "===", "__id__", "__send__",
    "class", "clone", "define_singleton_method", "display", "dup", "enum_for", "eql?", "equal?",
    "extend", "freeze", "frozen?", "hash", "inspect", "instance_eval", "instance_exec", "instance_of?",
    "instance_variable_defined?", "instance_variable_get", "instance_variable_set", "instance_variables", "is_a?", "itself", "kind_of?", "method",
    "methods", "nil?", "object_id", "private_methods", "protected_methods", "public_method", "public_methods", "public_send",
    "remove_instance_variable", "respond_to?", "send", "singleton_class", "singleton_method", "singleton_methods", "tap", "then",
    "to_enum", "to_s", "yield_self",
];

/// `Enumerable`'s public instance methods: what a class including it answers without `method_missing`.
pub(crate) const ENUMERABLE: &[&str] = &[
    "all?", "any?", "chain", "chunk", "chunk_while", "collect", "collect_concat", "compact",
    "count", "cycle", "detect", "drop", "drop_while", "each_cons", "each_entry", "each_slice",
    "each_with_index", "each_with_object", "entries", "filter", "filter_map", "find", "find_all", "find_index",
    "first", "flat_map", "grep", "grep_v", "group_by", "include?", "inject", "lazy",
    "map", "max", "max_by", "member?", "min", "min_by", "minmax", "minmax_by",
    "none?", "one?", "partition", "reduce", "reject", "reverse_each", "select", "slice_after",
    "slice_before", "slice_when", "sort", "sort_by", "sum", "take", "take_while", "tally",
    "to_a", "to_h", "to_set", "uniq", "zip",
];

/// `Comparable`'s.
pub(crate) const COMPARABLE: &[&str] = &[
    "<", "<=", "==", ">", ">=", "between?", "clamp",
];

/// `Kernel`'s private instance methods: what a receiverless call answers without `method_missing` (`block_given?`).
pub(crate) const KERNEL_PRIVATE: &[&str] = &[
    "Array", "Complex", "Float", "Hash", "Integer", "Pathname", "Rational", "String",
    "__callee__", "__dir__", "__method__", "`", "abort", "at_exit", "autoload", "autoload?",
    "binding", "block_given?", "caller", "caller_locations", "catch", "eval", "exec", "exit",
    "exit!", "fail", "fork", "format", "gem", "gem_original_require", "gets", "global_variables",
    "initialize_clone", "initialize_copy", "initialize_dup", "instance_variables_to_inspect", "iterator?", "lambda", "load", "local_variables",
    "loop", "open", "p", "pp", "print", "printf", "proc", "putc",
    "puts", "raise", "rand", "readline", "readlines", "require", "require_relative", "respond_to_missing?",
    "select", "set_trace_func", "sleep", "spawn", "sprintf", "srand", "syscall", "system",
    "test", "throw", "trace_var", "trap", "untrace_var", "warn",
];

/// ActiveRecord 8.1.4.1's public instance methods of `ActiveRecord::Base` that a plain Ruby 4.0.7 `Object` lacks
/// (`require "active_record"`, before any model is defined): what every model answers from its ancestors, without
/// `method_missing` (`association`, `has_changes_to_save?`, `previously_new_record?`).
pub(crate) const ACTIVE_RECORD_BASE: &[&str] = &[
    "[]", "[]=", "__callbacks", "_before_commit_callbacks", "_commit_callbacks", "_create_callbacks", "_destroy", "_destroy_callbacks",
    "_find_callbacks", "_has_attribute?", "_initialize_callbacks", "_last_transaction_return_status", "_last_transaction_return_status=", "_marshal_dump_7_1", "_new_record_before_last_commit", "_new_record_before_last_commit=",
    "_query_attribute", "_read_attribute", "_reflections", "_reflections?", "_rollback_callbacks", "_run_before_commit_callbacks", "_run_before_commit_callbacks!", "_run_commit_callbacks",
    "_run_commit_callbacks!", "_run_create_callbacks", "_run_create_callbacks!", "_run_destroy_callbacks", "_run_destroy_callbacks!", "_run_find_callbacks", "_run_find_callbacks!", "_run_initialize_callbacks",
    "_run_initialize_callbacks!", "_run_rollback_callbacks", "_run_rollback_callbacks!", "_run_save_callbacks", "_run_save_callbacks!", "_run_touch_callbacks", "_run_touch_callbacks!", "_run_update_callbacks",
    "_run_update_callbacks!", "_run_validate_callbacks", "_run_validate_callbacks!", "_run_validation_callbacks", "_run_validation_callbacks!", "_save_callbacks", "_touch_callbacks", "_update_callbacks",
    "_validate_callbacks", "_validation_callbacks", "_validators", "_validators?", "_write_attribute", "accessed_fields", "acts_like?", "aggregate_reflections",
    "aggregate_reflections?", "as_json", "assign_attributes", "association", "association_cached?", "attribute_aliases", "attribute_aliases?", "attribute_before_last_save",
    "attribute_change_to_be_saved", "attribute_changed?", "attribute_changed_in_place?", "attribute_for_inspect", "attribute_in_database", "attribute_method_patterns", "attribute_method_patterns?", "attribute_missing",
    "attribute_names", "attribute_present?", "attribute_previously_changed?", "attribute_previously_was", "attribute_was", "attribute_writer_missing", "attributes", "attributes=",
    "attributes_before_type_cast", "attributes_for_database", "attributes_in_database", "automatic_scope_inversing", "automatic_scope_inversing?", "automatically_invert_plural_associations", "automatically_invert_plural_associations?", "autosaving_belongs_to_for?",
    "becomes", "becomes!", "before_committed!", "blank?", "cache_key", "cache_key_with_version", "cache_timestamp_format", "cache_timestamp_format?",
    "cache_version", "cache_versioning", "cache_versioning?", "changed", "changed?", "changed_attribute_names_to_save", "changed_attributes", "changed_for_autosave?",
    "changes", "changes_applied", "changes_to_save", "ciphertext_for", "clear_attribute_changes", "clear_changes_information", "collection_cache_versioning", "collection_cache_versioning?",
    "column_for_attribute", "committed!", "connection_handler", "counter_cached_association_names", "counter_cached_association_names?", "custom_validation_context?", "decrement", "decrement!",
    "decrypt", "deep_dup", "default_connection_handler", "default_connection_handler?", "default_role", "default_role?", "default_scope_override", "default_scopes",
    "default_shard", "default_shard?", "defined_enums", "defined_enums?", "delete", "destroy", "destroy!", "destroy_association_async_batch_size",
    "destroy_association_async_job", "destroyed?", "destroyed_by_association", "destroyed_by_association=", "duplicable?", "encode_with", "encrypt", "encrypted_attribute?",
    "encrypted_attributes", "encrypted_attributes=", "encrypted_attributes?", "errors", "from_json", "full_inspect", "generate_token_for", "has_attribute?",
    "has_changes_to_save?", "id", "id=", "id?", "id_before_type_cast", "id_for_database", "id_in_database", "id_was",
    "in?", "include_root_in_json", "include_root_in_json?", "increment", "increment!", "init_attributes", "init_with", "init_with_attributes",
    "initialize_internals_callback", "instance_values", "instance_variable_names", "invalid?", "lock!", "lock_optimistically", "lock_optimistically?", "locking_enabled?",
    "logger", "logger?", "mark_for_destruction", "marked_for_destruction?", "marshal_load", "model_name", "nested_attributes_options", "nested_attributes_options?",
    "new_record?", "no_touching?", "normalize_attribute", "normalized_attributes", "normalized_attributes=", "normalized_attributes?", "param_delimiter=", "partial_inserts",
    "partial_inserts?", "partial_updates", "partial_updates?", "persisted?", "pluralize_table_names", "pluralize_table_names?", "populate_with_current_scope_attributes", "presence",
    "presence_in", "present?", "pretty_print", "previous_changes", "previously_new_record?", "previously_persisted?", "primary_key_prefix_type", "primary_key_prefix_type?",
    "primary_key_values_present?", "query_attribute", "read_attribute", "read_attribute_before_type_cast", "read_attribute_for_database", "read_attribute_for_serialization", "read_attribute_for_validation", "readonly!",
    "readonly?", "record_timestamps", "record_timestamps=", "record_timestamps?", "reload", "require_dependency", "respond_to_without_attributes?", "restore_attributes",
    "rolledback!", "run_callbacks", "save", "save!", "saved_change_to_attribute", "saved_change_to_attribute?", "saved_changes", "saved_changes?",
    "serializable_hash", "signed_id", "signed_id_verifier_secret", "signed_id_verifier_secret?", "skip_time_zone_conversion_for_attributes", "skip_time_zone_conversion_for_attributes?", "slice", "store_full_class_name",
    "store_full_class_name?", "store_full_sti_class", "store_full_sti_class?", "strict_loading!", "strict_loading?", "strict_loading_all?", "strict_loading_mode", "strict_loading_n_plus_one_only?",
    "table_name_prefix", "table_name_prefix?", "table_name_suffix", "table_name_suffix?", "time_zone_aware_attributes", "time_zone_aware_attributes?", "time_zone_aware_types", "time_zone_aware_types?",
    "to_json", "to_key", "to_model", "to_param", "to_partial_path", "to_query", "to_yaml", "toggle",
    "toggle!", "touch", "touch_later", "transaction", "trigger_transactional_callbacks?", "try", "try!", "type_for_attribute",
    "update", "update!", "update_attribute", "update_attribute!", "update_column", "update_columns", "valid?", "validate",
    "validate!", "validates_absence_of", "validates_acceptance_of", "validates_comparison_of", "validates_confirmation_of", "validates_exclusion_of", "validates_format_of", "validates_inclusion_of",
    "validates_length_of", "validates_numericality_of", "validates_presence_of", "validates_size_of", "validates_with", "validating_belongs_to_for?", "validation_context", "values_at",
    "will_save_change_to_attribute?", "with_lock", "with_transaction_returning_status", "write_attribute",
];
