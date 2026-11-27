# frozen_string_literal: true
# The `pagy` gem before 43 (9.3.5 -- the version maybe's Gemfile.lock pins):
# `Pagy::Backend#pagy`, the array extra's `#pagy_array`, the offset paginator
# they return, the overflow extra, and `Pagy::Frontend#pagy_url_for`.
#
# project.rs ships this file instead of pagy.rb when the app locks a pagy
# below 43, whose API is another one (`prev`/`series`/`vars`, `Pagy::Backend`
# and `Pagy::Frontend`, an OverflowError past the end). Every expected value
# in tests/emit_and_run/pagy9.rs is what the real gem answered
# (tests/pagy9_oracle/oracle.rb).
#
# WHAT IS MODELED
#
# - `pagy(scope, limit:, page:, items:)` and `pagy_array(array, ...)`, each
#   option optional. `limit:` and `page:` are read as the gem reads them:
#   a nil or a blank one is the default (20, and the request's `page`
#   param), anything else answers `to_i`, and one under 1 raises
#   Pagy::VariableError. `items:` is only kept, as `vars[:items]`.
# - `count page limit offset last pages from to in prev next`, `series`
#   (7 slots, ends on), and `vars[k]` for `count limit page items outset size`.
# - Past the end, Pagy::OverflowError, or with the overflow extra the mode
#   `Pagy::DEFAULT[:overflow]` names: `:last_page`, `:empty_page` or
#   `:exception` (project.rs replays the initializer's setting as
#   `Pagy.overflow =`).
# - `pagy_url_for(pagy, page)`: the request's path and its query pairs,
#   decoded and re-encoded as Rack's `build_nested_query` writes them, with
#   `page` replaced in place or appended.
#
# WHAT IS NOT, and how each is refused
#
# - Other options and paginators, `vars` but as `vars[<one of those keys>]`, the
#   frontend's nav helpers: refused at analysis (analyze/body: pagy).
# - A query key with `[` or `]` (a nested parameter): pagy_url_for raises
#   NotImplementedError rather than rebuilding the link differently. A
#   malformed `%` escape raises ArgumentError (Rack answers 400 for it).

class Pagy
  # `include Pagy::Backend` / `include Pagy::Frontend` resolve; the methods
  # are on ActionController::Base and ActionView::ViewHelpers below.
  module Backend
  end

  module Frontend
  end

  class VariableError < ArgumentError
  end

  class OverflowError < VariableError
  end

  HEX = "0123456789ABCDEF"

  # nil without the overflow extra, as the gem raises then; the extra's mode otherwise.
  @overflow = nil

  def self.overflow
    @overflow
  end

  def self.overflow=(mode)
    @overflow = mode
  end

  # `Pagy::DEFAULT[:limit]`, which an initializer may set.
  @default_limit = 20

  def self.default_limit
    @default_limit
  end

  def self.default_limit=(limit)
    @default_limit = limit
  end

  # `raw_limit` and `raw_page` are as the caller passed them, nil for none.
  def initialize(count, raw_limit, raw_page, items, path, query)
    @items = items
    @var_limit = raw_limit.nil? || raw_limit == "" ? Pagy.default_limit : raw_limit
    @var_page = raw_page.nil? || raw_page == "" ? 1 : raw_page
    @path = path
    @query = query
    limit = Pagy.to_var(raw_limit, Pagy.default_limit, "limit", 1)
    page = Pagy.to_var(raw_page, 1, "page", 1)
    raise VariableError, "expected :count >= 0; got #{count}" if count < 0
    last_page = (count + limit - 1) / limit
    last_page = 1 if last_page < 1
    mode = Pagy.overflow
    empty = false
    if page > last_page
      raise OverflowError, "expected :page in 1..#{last_page}; got #{page}" if mode.nil? || mode == :exception
      raise VariableError, "expected :overflow to be in [:last_page, :empty_page, :exception]; got #{mode.inspect}" unless mode == :last_page || mode == :empty_page
      empty = mode == :empty_page
      page = last_page if mode == :last_page
    end
    offset = limit * (page - 1)
    from_row = offset + 1
    from_row = count if count < from_row
    to_row = offset + limit
    to_row = count if count < to_row
    in_page = to_row - from_row + 1
    in_page = count if count < in_page
    before = page == 1 ? nil : page - 1
    after = page == last_page ? nil : page + 1
    if empty
      from_row = 0
      to_row = 0
      in_page = 0
      before = last_page
      after = nil
    end
    @count = count
    @limit = limit
    @page = page
    @offset = offset
    @last = last_page
    @from = from_row
    @to = to_row
    @in = in_page
    @prev = before
    @next = after
    @empty = empty
  end

  def count
    @count
  end

  def limit
    @limit
  end

  def page
    @page
  end

  def offset
    @offset
  end

  def last
    @last
  end

  def pages
    @last
  end

  def from
    @from
  end

  def to
    @to
  end

  def in
    @in
  end

  def prev
    @prev
  end

  def next
    @next
  end

  # The keys the analysis admits in `vars[...]`: what the caller passed
  # (a blank one replaced by the default) and the computed count. The
  # gem's other keys (`ends`, `page_param`, `count_args`, `overflow`)
  # hold values of other types and are refused there.
  def vars
    { count: @count, limit: @var_limit, page: @var_page, items: @items, outset: 0, size: 7 }
  end

  # The gem's 7 slots with the ends kept: the current page as a String,
  # `:gap` where pages are skipped. Past the end with `:empty_page`, the
  # last page's series with no current page.
  def series
    last_page = @last
    current = @empty ? last_page : @page
    out = []
    if 7 >= last_page
      n = 1
      while n <= last_page
        out.push(n)
        n += 1
      end
    else
      start = current - 3
      start = 1 if current <= 3
      start = last_page - 6 if current > last_page - 4
      n = start
      while n < start + 7
        out.push(n)
        n += 1
      end
      out[0] = 1
      out[1] = :gap unless out[1] == 2
      out[5] = :gap unless out[5] == last_page - 1
      out[6] = last_page
    end
    return out if @empty
    at = out.index(current)
    out[at] = current.to_s unless at.nil?
    out
  end

  def url_for(page)
    pairs = Pagy.query_pairs(@query)
    value = page.to_s
    found = false
    pairs.each do |pair|
      if pair[0] == "page"
        pair[1] = value
        found = true
      end
    end
    pairs.push(["page", value]) unless found
    parts = pairs.map { |pair| pair[1].nil? ? Pagy.escape(pair[0].to_s) : Pagy.escape(pair[0].to_s) + "=" + Pagy.escape(pair[1].to_s) }
    @path + "?" + parts.join("&")
  end

  # A caller's `limit:` / `page:` as the gem's `assign_vars` and
  # `assign_and_check` read it: nil or "" is the default, else `to_i`.
  def self.to_var(raw, default, name, min)
    return default if raw.nil? || raw == ""
    value = -1
    value = raw if raw.is_a?(Integer)
    value = raw.to_i if raw.is_a?(String)
    raise VariableError, "expected :#{name} >= #{min}; got #{raw.inspect}" if value < min
    value
  end

  # Not through `to_var`'s blank default: the gem's `(params[:page] || 1).to_i` makes `?page=` a 0, which it refuses.
  def self.page_var(option, param)
    return option unless option.nil?
    return 1 if param.nil?
    param.to_s.to_i
  end

  # Rack's `parse_nested_query` for a flat query: decoded pairs, a repeated
  # key keeping its first position and its last value, a key without `=`
  # holding nil.
  def self.query_pairs(query)
    pairs = []
    query.split("&").each do |segment|
      next if segment.empty?
      eq = segment.index("=")
      key = decode(eq.nil? ? segment : segment[0, eq].to_s)
      raise NotImplementedError, "pagy_url_for: the nested query parameter #{key.inspect} is not modeled" if key.include?("[") || key.include?("]")
      next if key.empty?
      value = eq.nil? ? nil : decode(segment[eq + 1, segment.length].to_s)
      existing = pairs.find { |pair| pair[0] == key }
      if existing.nil?
        pairs.push([key, value])
      else
        existing[1] = value
      end
    end
    pairs
  end

  def self.decode(raw)
    bytes = []
    i = 0
    while i < raw.bytesize
      b = raw.getbyte(i)
      if b == 43
        bytes.push(32)
        i += 1
      elsif b == 37
        hi = hex_value(raw.getbyte(i + 1))
        lo = hex_value(raw.getbyte(i + 2))
        raise ArgumentError, "invalid %-encoding (#{raw})" if hi < 0 || lo < 0
        bytes.push(hi * 16 + lo)
        i += 3
      else
        bytes.push(b)
        i += 1
      end
    end
    bytes.pack("C*").force_encoding("UTF-8")
  end

  def self.hex_value(b)
    return -1 if b.nil?
    return b - 48 if b >= 48 && b <= 57
    return b - 55 if b >= 65 && b <= 70
    return b - 87 if b >= 97 && b <= 102
    -1
  end

  # `URI.encode_www_form_component`: unreserved `*-._` and alphanumerics
  # kept, a space as `+`, every other byte `%XX`.
  def self.escape(text)
    out = +""
    i = 0
    while i < text.bytesize
      b = text.getbyte(i).to_i
      if (b >= 48 && b <= 57) || (b >= 65 && b <= 90) || (b >= 97 && b <= 122) || b == 42 || b == 45 || b == 46 || b == 95
        out << b.chr
      elsif b == 32
        out << "+"
      else
        out << "%" << HEX[b >> 4].to_s << HEX[b & 15].to_s
      end
      i += 1
    end
    out
  end
end

module ActionController
  class Base
    # `Pagy::Backend#pagy`: `[pagy, records]`. `options` is a Hash, not
    # keywords, as pagy.rb's is, so that a wrapper's `super` reaches it the
    # way a direct call does.
    def pagy(collection, options = {})
      path = request_path.split("?", 2)[0].to_s
      query = query_string
      current = request
      unless current.nil?
        path = current.path
        query = current.query_string
      end
      paginator = Pagy.new(collection.count, options[:limit], Pagy.page_var(options[:page], params["page"]), options[:items], path, query)
      [paginator, collection.offset(paginator.offset).limit(paginator.limit)]
    end

    # The array extra's `pagy_array`.
    def pagy_array(array, options = {})
      path = request_path.split("?", 2)[0].to_s
      query = query_string
      current = request
      unless current.nil?
        path = current.path
        query = current.query_string
      end
      paginator = Pagy.new(array.length, options[:limit], Pagy.page_var(options[:page], params["page"]), options[:items], path, query)
      [paginator, array[paginator.offset, paginator.limit] || []]
    end
  end
end

module ActionView
  module ViewHelpers
    # `Pagy::Frontend#pagy_url_for`, which reads the request the paginator was built from.
    def self.pagy_url_for(pagy, page)
      pagy.url_for(page)
    end
  end
end
