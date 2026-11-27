# frozen_string_literal: true
# The storage boundary for PostgreSQL array columns (`array: true`) on a
# PostgreSQL database, where the column is a real array (`text[]`,
# `integer[]`). It replaces array_column.rb, the SQLite JSON codec, in
# the PostgreSQL tree (`project::apply_postgresql_tree`), with the same
# API: the record keeps the decoded Array, so an in-place `labels << x`
# is what the next save writes.
#
# `dump` writes PostgreSQL's array input format, every element quoted
# (`{"a","b,c"}`), so an empty string, `NULL`, a brace, a comma or a
# space survive; a nil element is an unquoted NULL. The caller escapes
# the result as an SQL string (`Db.escape_string_opt`), so a `'` is
# doubled after the array layer's `\` and `"` are escaped.
#
# `load_*` read the array output format the server answers with:
# elements quoted only when they need it, `\` escaping inside quotes,
# an unquoted NULL (any case) a NULL element. A multi-dimensional array
# or one with explicit bounds (`[0:1]={…}`) is not carried and raises.
#
# Limit: a NULL element (one raw SQL wrote, `ARRAY['a', NULL]`) reads
# as "" or 0, as the SQLite JSON lane maps a JSON null, where Rails
# gives nil. The column is `Array[String]` / `Array[Integer]`, and a
# Spinel Array of either cannot hold nil.
# Char-index loops with typed locals, no regex, so Spinel types them.

module ArrayColumn
  def self.load_ints(text)
    return nil if text.nil?
    out = []
    parse(text).each { |element| out << element.to_i }
    out
  end

  def self.load_strings(text)
    return nil if text.nil?
    parse(text)
  end

  def self.dump(value)
    return nil if value.nil?
    out = +"{"
    first = true
    value.each do |element|
      out << "," unless first
      first = false
      if element.nil?
        out << "NULL"
      else
        out << "\""
        s = element.to_s
        i = 0
        while i < s.length
          c = s[i]
          out << "\\" if c == "\\" || c == "\""
          out << c
          i += 1
        end
        out << "\""
      end
    end
    out << "}"
    out
  end

  def self.parse(text)
    out = []
    n = text.length
    if n < 2 || text[0] != "{" || text[n - 1] != "}"
      raise ArgumentError, "ArrayColumn: not a one-dimensional PostgreSQL array: #{text}"
    end
    return out if n == 2
    i = 1
    while i < n
      c = text[i]
      if c == "{"
        raise ArgumentError, "ArrayColumn: a multi-dimensional PostgreSQL array is not carried: #{text}"
      end
      element = +""
      if c == "\""
        i += 1
        while i < n && text[i] != "\""
          i += 1 if text[i] == "\\"
          element << text[i]
          i += 1
        end
        i += 1
        out << element
      else
        while i < n && text[i] != "," && text[i] != "}"
          element << text[i]
          i += 1
        end
        out << (element.upcase == "NULL" ? "" : element)
      end
      i += 1
    end
    out
  end
end
