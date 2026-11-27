# Hand-written — ActiveRecord::Base is not yet on ELIXIR_RUNTIME
# (while→recursion / instance mutation still growing), but models already
# call `ActiveRecord.Reflection.AssociationReflection.new/2` from the
# lowered `reflect_on_association` surface (#818 / #832). Ship the thin
# struct the call site needs until Base itself transpiles.
defmodule ActiveRecord.Reflection.AssociationReflection do
  defstruct [:name, :counter_cache_column, :macro, foreign_key: "", polymorphic: false]

  def new(name, counter_cache_column) do
    %__MODULE__{name: name, counter_cache_column: counter_cache_column}
  end

  def new(name, counter_cache_column, macro, foreign_key, polymorphic) do
    %__MODULE__{name: name, counter_cache_column: counter_cache_column, macro: macro, foreign_key: foreign_key, polymorphic: polymorphic}
  end

  def name(%__MODULE__{name: name}), do: name
  def counter_cache_column(%__MODULE__{counter_cache_column: c}), do: c
  def klass(_ref), do: nil
  def macro(%__MODULE__{macro: m}), do: m
  def belongs_to?(%__MODULE__{macro: m}), do: m == :belongs_to
  def foreign_key(%__MODULE__{foreign_key: k}), do: k
  def polymorphic?(%__MODULE__{polymorphic: p}), do: p
  def options(%__MODULE__{polymorphic: true}), do: %{polymorphic: true}
  def options(_ref), do: %{}
  def foreign_type(%__MODULE__{polymorphic: true, name: n}), do: "#{n}_type"
  def foreign_type(_ref), do: nil
end
