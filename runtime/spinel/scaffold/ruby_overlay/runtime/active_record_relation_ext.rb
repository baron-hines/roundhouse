# frozen_string_literal: true
# CRuby-only ActiveRecord::Relation extensions: enumerable terminals,
# record-valued IN lists, and `order_values` as the Arel orderings that
# live beside them on this overlay (`arel_predicates.rb`).
#
# Lives on the CRuby overlay, not shared runtime/ruby/active_record/
# relation.rb: the IN-list coercion dispatches on
# is_a?(ActiveRecord::Base) — exactly the shape the typed shared
# runtime refuses (see the monomorphic-API rule). When lobsters comes
# up on another target, that target grows its own variant.
# (`Relation#+` lived here too until it moved to the shared runtime.)
module ActiveRecord
  class Relation
    # Enumerable terminal Rails relations answer via to_a delegation;
    # lobsters' Comment.arrange_for_user groups its ordered relation by
    # parent id.
    def group_by(&block)
      to_a.group_by(&block)
    end

    # query_methods.rb `order_values`: each `order` argument as Rails'
    # `preprocess_order_args` keeps it. A Symbol or a Hash key is the
    # Ascending/Descending of the column's attribute, a node is the node,
    # a String the String as written. The relation keeps rendered SQL
    # (`@orders`), so the terms an `order` call rendered remember what it
    # was given, keyed by the very String it pushed: a term rebuilt from
    # SQL (`reverse_order`'s) answers its SQL.
    def order_values
      known = @order_nodes || {}
      @orders.flat_map { |term| known[term] || [term] }
    end

    alias_method :order_term_sql, :order_term

    # The ordering's SQL, as before, and what Rails holds for it. A fresh
    # Hash each time: a spawned relation shares the receiver's ivars.
    def order_term(part)
      sql = order_term_sql(part)
      known = {}.compare_by_identity
      known.update(@order_nodes) unless @order_nodes.nil?
      known[sql] = order_nodes(part)
      @order_nodes = known
      sql
    end

    alias_method :order_parts_sql!, :order_parts!

    # Rails flattens `order` / `reorder` arguments, so a list read back
    # from `order_values` orders a relation again.
    def order_parts!(parts)
      order_parts_sql!(parts.flatten)
    end

    private

    def order_nodes(part)
      if part.is_a?(Symbol)
        [order_attribute(part).asc]
      elsif part.is_a?(Hash)
        part.flat_map do |column, direction|
          if direction.is_a?(Hash)
            direction.map { |field, dir| order_node(Arel::Table.new(column.to_s).attribute(field.to_s), dir) }
          else
            [order_node(order_attribute(column), direction)]
          end
        end
      else
        [part]
      end
    end

    # `order_column`: the model's column, or a named table's
    # (`order("stories.id": :desc)`).
    def order_attribute(column)
      table, _, name = column.to_s.rpartition(".")
      table.empty? ? @model.arel_table.attribute(name) : Arel::Table.new(table).attribute(name)
    end

    def order_node(attribute, direction)
      direction.to_s.downcase == "desc" ? attribute.desc : attribute.asc
    end

    # Rails casts an ActiveRecord object used as a condition value to its
    # id (`where(story_id: [<Story rows>, 7])` → `IN (3, 5, 7)`). The
    # shared escape_list feeds values straight to the adapter, which only
    # knows scalars — so the record→id cast happens here, on the overlay.
    def escape_list(vals)
      out = []
      vals.each do |v|
        v = v.id if v.is_a?(ActiveRecord::Base)
        out << ActiveRecord.adapter.escape_value(v)
      end
      out.join(", ")
    end
  end
end
