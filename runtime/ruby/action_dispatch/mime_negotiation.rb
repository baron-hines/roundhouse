# frozen_string_literal: true
# actionpack 8.1's `ActionDispatch::Http::MimeNegotiation#
# should_apply_vary_header?`: whether the response format was
# negotiated from the request's Accept header, which is when Rails'
# `render` answers `Vary: Accept` (ActionController::Rendering
# `_set_vary_header`, Rails 7.1 and later). A cache that keyed only on
# the URL would otherwise hand an HTML page to a JSON client.
#
# RUBY-FAMILY ONLY: both ruby-family dispatchers (the CRuby overlay's
# `main.rb`, the spinel scaffold's `main.rb`) call it once per request
# and hand the answer to the controller (`Base#vary_on_accept`). Plain
# Strings in, so the two dispatchers' different request shapes share
# it. `config.action_dispatch.ignore_accept_header` is not modeled: it
# defaults to false, which is what this answers for.
module ActionDispatch
  module MimeNegotiation
    # `format_param`: the request has a `format` parameter (a path
    # extension, `?format=`, or a route default), which Rails reads
    # instead of the Accept header. The other three are the request's
    # Accept, X-Requested-With and Content-Type headers, "" when absent.
    def self.should_apply_vary_header?(accept, x_requested_with, content_type, format_param)
      return false if format_param
      accept_present = !accept.strip.empty?
      # `xhr?`: an X-Requested-With naming XMLHttpRequest, any case.
      xhr = x_requested_with.downcase.include?("xmlhttprequest")
      # `valid_accept_header`: an XHR with an Accept or a body type, or
      # an Accept that is not a browser's (a list with `*/*` beside
      # another type: Rails' BROWSER_LIKE_ACCEPTS).
      return true if xhr && (accept_present || !content_type.empty?)
      accept_present && !accept.match?(/,\s*\*\/\*|\*\/\*\s*,/)
    end

    # Whether Rails' `request.formats.first` is JSON, which is when
    # `ActionDispatch::PublicExceptions` (the production exceptions app)
    # answers an error status with `{"status":404,"error":"Not Found"}`
    # instead of the static HTML page. `formats` reads, in order: the
    # `format` parameter (a routed path's extension, `?format=`, a
    # route's own format; `format_param`, "" when absent), the Accept
    # header when it is one Rails trusts (see above), and the path's own
    # extension (`extension`, "" when none). `formats.first` of an
    # Accept list is its highest-q type, the earliest on a tie.
    def self.json_format?(format_param, extension, accept, x_requested_with, content_type)
      return format_param == "json" unless format_param.empty?
      if should_apply_vary_header?(accept, x_requested_with, content_type, false)
        preferred = accept.strip.empty? ? content_type : preferred_type(accept)
        return json_type?(preferred)
      end
      extension == "json"
    end

    # Rails' `request.formats.first` as an extension name, before a route
    # is chosen (what a request constraint reads): the path's extension
    # (`extension`, "" when none), the Accept header when Rails trusts
    # it, "js" for an XHR, else "html".
    def self.format_name(extension, accept, x_requested_with, content_type)
      return extension unless extension.empty?
      if should_apply_vary_header?(accept, x_requested_with, content_type, false)
        preferred = accept.strip.empty? ? content_type : preferred_type(accept)
        symbol = Mime::Type.symbol_for(preferred.split(";", 2)[0].to_s.strip.downcase)
        return symbol.to_s unless symbol.nil?
      end
      return "js" if x_requested_with.downcase.include?("xmlhttprequest")
      "html"
    end

    # The Accept list's highest-q media type (`Mime::Type.parse`, then
    # `first`), without its parameters.
    def self.preferred_type(accept)
      best = ""
      best_q = -1.0
      accept.split(",").each do |entry|
        parts = entry.split(";")
        q = 1.0
        parts.each do |part|
          param = part.strip
          q = param[2, param.length].to_s.to_f if param.start_with?("q=")
        end
        if q > best_q
          best = parts.empty? ? "" : parts[0].to_s.strip
          best_q = q
        end
      end
      best
    end

    # The formats a `format.any(*list)` arm registers, as the names
    # `Base#negotiate_format` compares (`:html` and "html" alike).
    def self.format_names(list)
      list.map { |f| f.to_s }
    end

    # The media types Rails registers for `:json`.
    def self.json_type?(media_type)
      type = media_type.split(";", 2)[0].to_s.strip.downcase
      type == "application/json" || type == "text/x-json" || type == "application/jsonrequest"
    end

    # ---- Accept parsing (`Base#negotiate_format` reads its answer) --------
    #
    # Module functions with ONE counter-stepped loop each and no early return
    # inside it: the shape `functionalize/while_to_recursion` lowers to
    # recursion for the Elixir library emit.

    # `Request#formats` from the `Accept` header, as format names in
    # preference order (highest q first, ties in header order). A blank
    # header, or a browser's list that ends in or contains `*/*` beside
    # other types (Rails' BROWSER_LIKE_ACCEPTS), is html alone. The
    # XMLHttpRequest case (`X-Requested-With`) is not modeled.
    #
    # Cost: ranking is quadratic in the item count (an item's slot counts
    # the items that outrank it), and a header's item count is the
    # client's to choose. So ranking runs on the DISTINCT recognised
    # formats only, never on the raw items: items of an unregistered type,
    # and every item but the best one of a format (highest q, earliest on
    # a tie), cannot change which format comes first, so dropping them
    # changes no outcome. That leaves at most one item per registered
    # format plus `*/*` (about forty), however long the header, and every
    # earlier step is linear. Rails ranks them all and gets the same order.
    def self.accepted_formats(accept)
      return ["html"] if accept.strip.empty?
      squeezed = accept.gsub(" ", "").gsub("\t", "").gsub("\n", "").gsub("\r", "")
      return ["html"] if squeezed.include?(",*/*") || squeezed.include?("*/*,")
      items = expand_trailing_stars(accept.split(","))
      all_names = accept_item_names(items)
      all_qs = accept_item_qs(items)
      keep = best_item_flags(all_names, all_qs)
      names = flagged_names(all_names, keep)
      qs = flagged_qs(all_qs, keep)
      # An item's slot is the number of items that outrank it.
      ranks = outranking_counts(qs)
      ordered_non_empty(names, ranks)
    end

    # `text/*` and `application/*` expand to the registered types they
    # match, in Rails' registration order, each keeping the item's
    # parameters (so its q): Rails' `parse_trailing_star`. Only those two
    # prefixes, and case-sensitively, as upstream's TRAILING_STAR_REGEXP.
    def self.expand_trailing_stars(items)
      out = []
      i = 0
      while i < items.length
        item = items[i].to_s
        star = trailing_star_mimes(item.strip)
        if star.empty?
          out << item
        else
          rest = item.include?(";") ? ";" + item.split(";", 2)[1].to_s : ""
          out.concat(star_items(star, rest))
        end
        i += 1
      end
      out
    end

    def self.star_items(mimes, rest)
      items = []
      j = 0
      while j < mimes.length
        items << mimes[j].to_s + rest
        j += 1
      end
      items
    end

    # One flag per item: true for the best item of each recognised format.
    def self.best_item_flags(names, qs)
      best = []
      i = 0
      while i < names.length
        name = names[i].to_s
        if !name.empty?
          slot = best_slot(names, best, name)
          if slot < 0
            best << i
          elsif qs[i] > qs[best[slot]]
            best[slot] = i
          end
        end
        i += 1
      end
      flags_for(best, names.length)
    end

    # Where in `best` the item for `name` sits, or -1.
    def self.best_slot(names, best, name)
      found = -1
      k = 0
      while k < best.length
        found = k if names[best[k]] == name && found < 0
        k += 1
      end
      found
    end

    # One loop, like every helper here: two would not lower to Elixir.
    def self.flags_for(indexes, count)
      flags = []
      i = 0
      while i < count
        flags << indexes.include?(i)
        i += 1
      end
      flags
    end

    def self.flagged_names(names, flags)
      out = []
      i = 0
      while i < names.length
        out << names[i] if flags[i]
        i += 1
      end
      out
    end

    def self.flagged_qs(qs, flags)
      out = []
      i = 0
      while i < qs.length
        out << qs[i] if flags[i]
        i += 1
      end
      out
    end

    # The mime types `Mime::Type.parse("<prefix>/*")` answers under
    # actionpack 8.1.3.1 + turbo-rails 2.0.23, in order (a type matches
    # when its string or a synonym contains the prefix: `text/xml` and
    # `text/x-json` put xml and json under `text`; turbo_stream registers
    # last).
    def self.trailing_star_mimes(item)
      if item.start_with?("text/*")
        return ["text/html", "text/plain", "text/javascript", "text/css", "text/calendar",
                "text/csv", "text/vcard", "text/vtt", "text/markdown", "application/xml",
                "application/x-yaml", "application/json", "text/vnd.turbo-stream.html"]
      end
      if item.start_with?("application/*")
        return ["text/html", "text/javascript", "application/xml", "application/rss+xml",
                "application/atom+xml", "application/x-yaml", "application/x-www-form-urlencoded",
                "application/json", "application/pdf", "application/zip", "application/gzip"]
      end
      []
    end

    def self.accept_item_names(items)
      names = []
      i = 0
      while i < items.length
        names << accept_item_name(items[i].to_s)
        i += 1
      end
      names
    end

    def self.accept_item_qs(items)
      qs = []
      i = 0
      while i < items.length
        qs << accept_item_q(items[i].to_s)
        i += 1
      end
      qs
    end

    def self.accept_item_name(item)
      # Case-sensitive, as Rails' registry lookup is: `TEXT/HTML` is no type.
      mime = item.split(";")[0].to_s.strip
      return "*/*" if mime == "*/*"
      format_name_for_mime(mime)
    end

    # "text/vnd.turbo-stream.html" -> "turbo_stream"; "" for a type the
    # registry does not know.
    def self.format_name_for_mime(mime)
      return "" unless Mime::Type.valid?(mime)
      symbol = Mime::Type.lookup(mime).symbol
      return "" if symbol.nil?
      symbol.to_s
    end

    def self.accept_item_q(item)
      parts = item.split(";")
      given = []
      j = 1
      while j < parts.length
        param = parts[j].to_s.strip
        given << param[2, 16].to_s.to_f if param.start_with?("q=")
        j += 1
      end
      return 1.0 if given.empty?
      given[given.length - 1]
    end

    def self.outranking_counts(qs)
      ranks = []
      k = 0
      while k < qs.length
        ranks << outranking_count(qs, k)
        k += 1
      end
      ranks
    end

    def self.outranking_count(qs, at)
      outranking = []
      j = 0
      while j < qs.length
        outranking << j if qs[j] > qs[at] || (qs[j] == qs[at] && j < at)
        j += 1
      end
      outranking.length
    end

    # The names in rank order, unknown types (empty names) left out.
    def self.ordered_non_empty(names, ranks)
      ordered = []
      r = 0
      while r < names.length
        name = names[rank_index(ranks, r)].to_s
        ordered << name unless name.empty?
        r += 1
      end
      ordered
    end

    def self.rank_index(ranks, rank)
      found = []
      j = 0
      while j < ranks.length
        found << j if ranks[j] == rank
        j += 1
      end
      return 0 if found.empty?
      found[0]
    end
  end
end
