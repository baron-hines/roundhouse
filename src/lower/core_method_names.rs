//! Public instance methods of Ruby 3.4.11, sorted, generated from Ruby itself:
//! `Object.public_instance_methods`, and per class those of Hash, Array,
//! String, Symbol, Integer and Float that Object does not have.

pub(crate) const OBJECT_METHODS: &[&str] = &[
    "!", "!=", "!~", "<=>", "==", "===", "__id__", "__send__",
    "class", "clone", "define_singleton_method", "display", "dup", "enum_for", "eql?", "equal?",
    "extend", "freeze", "frozen?", "hash", "inspect", "instance_eval", "instance_exec", "instance_of?",
    "instance_variable_defined?", "instance_variable_get", "instance_variable_set", "instance_variables", "is_a?", "itself", "kind_of?", "method",
    "methods", "nil?", "object_id", "private_methods", "protected_methods", "public_method", "public_methods", "public_send",
    "remove_instance_variable", "respond_to?", "send", "singleton_class", "singleton_method", "singleton_methods", "tap", "then",
    "to_enum", "to_s", "yield_self",
];

const HASH_METHODS: &[&str] = &[
    "<", "<=", ">", ">=", "[]", "[]=", "all?", "any?",
    "assoc", "chain", "chunk", "chunk_while", "clear", "collect", "collect_concat", "compact",
    "compact!", "compare_by_identity", "compare_by_identity?", "count", "cycle", "deconstruct_keys", "default", "default=",
    "default_proc", "default_proc=", "delete", "delete_if", "detect", "dig", "drop", "drop_while",
    "each", "each_cons", "each_entry", "each_key", "each_pair", "each_slice", "each_value", "each_with_index",
    "each_with_object", "empty?", "entries", "except", "fetch", "fetch_values", "filter", "filter!",
    "filter_map", "find", "find_all", "find_index", "first", "flat_map", "flatten", "grep",
    "grep_v", "group_by", "has_key?", "has_value?", "include?", "inject", "invert", "keep_if",
    "key", "key?", "keys", "lazy", "length", "map", "max", "max_by",
    "member?", "merge", "merge!", "min", "min_by", "minmax", "minmax_by", "none?",
    "one?", "partition", "rassoc", "reduce", "rehash", "reject", "reject!", "replace",
    "reverse_each", "select", "select!", "shift", "size", "slice", "slice_after", "slice_before",
    "slice_when", "sort", "sort_by", "store", "sum", "take", "take_while", "tally",
    "to_a", "to_h", "to_hash", "to_proc", "to_set", "transform_keys", "transform_keys!", "transform_values",
    "transform_values!", "uniq", "update", "value?", "values", "values_at", "zip",
];

const ARRAY_METHODS: &[&str] = &[
    "&", "*", "+", "-", "<<", "[]", "[]=", "all?",
    "any?", "append", "assoc", "at", "bsearch", "bsearch_index", "chain", "chunk",
    "chunk_while", "clear", "collect", "collect!", "collect_concat", "combination", "compact", "compact!",
    "concat", "count", "cycle", "deconstruct", "delete", "delete_at", "delete_if", "detect",
    "difference", "dig", "drop", "drop_while", "each", "each_cons", "each_entry", "each_index",
    "each_slice", "each_with_index", "each_with_object", "empty?", "entries", "fetch", "fetch_values", "fill",
    "filter", "filter!", "filter_map", "find", "find_all", "find_index", "first", "flat_map",
    "flatten", "flatten!", "grep", "grep_v", "group_by", "include?", "index", "inject",
    "insert", "intersect?", "intersection", "join", "keep_if", "last", "lazy", "length",
    "map", "map!", "max", "max_by", "member?", "min", "min_by", "minmax",
    "minmax_by", "none?", "one?", "pack", "partition", "permutation", "pop", "prepend",
    "product", "push", "rassoc", "reduce", "reject", "reject!", "repeated_combination", "repeated_permutation",
    "replace", "reverse", "reverse!", "reverse_each", "rindex", "rotate", "rotate!", "sample",
    "select", "select!", "shift", "shuffle", "shuffle!", "size", "slice", "slice!",
    "slice_after", "slice_before", "slice_when", "sort", "sort!", "sort_by", "sort_by!", "sum",
    "take", "take_while", "tally", "to_a", "to_ary", "to_h", "to_set", "transpose",
    "union", "uniq", "uniq!", "unshift", "values_at", "zip", "|",
];

const STRING_METHODS: &[&str] = &[
    "%", "*", "+", "+@", "-@", "<", "<<", "<=",
    "=~", ">", ">=", "[]", "[]=", "append_as_bytes", "ascii_only?", "b",
    "between?", "byteindex", "byterindex", "bytes", "bytesize", "byteslice", "bytesplice", "capitalize",
    "capitalize!", "casecmp", "casecmp?", "center", "chars", "chomp", "chomp!", "chop",
    "chop!", "chr", "clamp", "clear", "codepoints", "concat", "count", "crypt",
    "dedup", "delete", "delete!", "delete_prefix", "delete_prefix!", "delete_suffix", "delete_suffix!", "downcase",
    "downcase!", "dump", "each_byte", "each_char", "each_codepoint", "each_grapheme_cluster", "each_line", "empty?",
    "encode", "encode!", "encoding", "end_with?", "force_encoding", "getbyte", "grapheme_clusters", "gsub",
    "gsub!", "hex", "include?", "index", "insert", "intern", "length", "lines",
    "ljust", "lstrip", "lstrip!", "match", "match?", "next", "next!", "oct",
    "ord", "partition", "prepend", "replace", "reverse", "reverse!", "rindex", "rjust",
    "rpartition", "rstrip", "rstrip!", "scan", "scrub", "scrub!", "setbyte", "size",
    "slice", "slice!", "split", "squeeze", "squeeze!", "start_with?", "strip", "strip!",
    "sub", "sub!", "succ", "succ!", "sum", "swapcase", "swapcase!", "to_c",
    "to_f", "to_i", "to_r", "to_str", "to_sym", "tr", "tr!", "tr_s",
    "tr_s!", "undump", "unicode_normalize", "unicode_normalize!", "unicode_normalized?", "unpack", "unpack1", "upcase",
    "upcase!", "upto", "valid_encoding?",
];

const SYMBOL_METHODS: &[&str] = &[
    "<", "<=", "=~", ">", ">=", "[]", "between?", "capitalize",
    "casecmp", "casecmp?", "clamp", "downcase", "empty?", "encoding", "end_with?", "id2name",
    "intern", "length", "match", "match?", "name", "next", "size", "slice",
    "start_with?", "succ", "swapcase", "to_proc", "to_sym", "upcase",
];

const INTEGER_METHODS: &[&str] = &[
    "%", "&", "*", "**", "+", "+@", "-", "-@",
    "/", "<", "<<", "<=", ">", ">=", ">>", "[]",
    "^", "abs", "abs2", "allbits?", "angle", "anybits?", "arg", "between?",
    "bit_length", "ceil", "ceildiv", "chr", "clamp", "coerce", "conj", "conjugate",
    "denominator", "digits", "div", "divmod", "downto", "even?", "fdiv", "finite?",
    "floor", "gcd", "gcdlcm", "i", "imag", "imaginary", "infinite?", "integer?",
    "lcm", "magnitude", "modulo", "negative?", "next", "nobits?", "nonzero?", "numerator",
    "odd?", "ord", "phase", "polar", "positive?", "pow", "pred", "quo",
    "rationalize", "real", "real?", "rect", "rectangular", "remainder", "round", "singleton_method_added",
    "size", "step", "succ", "times", "to_c", "to_f", "to_i", "to_int",
    "to_r", "truncate", "upto", "zero?", "|", "~",
];

const FLOAT_METHODS: &[&str] = &[
    "%", "*", "**", "+", "+@", "-", "-@", "/",
    "<", "<=", ">", ">=", "abs", "abs2", "angle", "arg",
    "between?", "ceil", "clamp", "coerce", "conj", "conjugate", "denominator", "div",
    "divmod", "fdiv", "finite?", "floor", "i", "imag", "imaginary", "infinite?",
    "integer?", "magnitude", "modulo", "nan?", "negative?", "next_float", "nonzero?", "numerator",
    "phase", "polar", "positive?", "prev_float", "quo", "rationalize", "real", "real?",
    "rect", "rectangular", "remainder", "round", "singleton_method_added", "step", "to_c", "to_f",
    "to_i", "to_int", "to_r", "truncate", "zero?",
];

const CORE_VALUE_CLASSES: &[(&str, &[&str])] = &[
    ("Hash", HASH_METHODS),
    ("Array", ARRAY_METHODS),
    ("String", STRING_METHODS),
    ("Symbol", SYMBOL_METHODS),
    ("Integer", INTEGER_METHODS),
    ("Float", FLOAT_METHODS),
];

pub(crate) fn defined_by_every_object(name: &str) -> bool {
    OBJECT_METHODS.binary_search(&name).is_ok()
}

pub(crate) fn core_value_definers(name: &str) -> Vec<&'static str> {
    CORE_VALUE_CLASSES
        .iter()
        .filter(|(_, methods)| methods.binary_search(&name).is_ok())
        .map(|(class, _)| *class)
        .collect()
}
