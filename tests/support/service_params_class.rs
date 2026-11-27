//! discourse's `Service::Base.params`: a class macro makes
//! `Class.new(base).tap { it.class_eval(&block) }` and names it with
//! `const_set`. The call's block is the new class's body, so a `def` in
//! it is the class's method and a `private` before it applies. A base the
//! call passes may be a class another call made. One contract for the
//! interpreted and native lanes.

use super::emit_and_run;

const STEPS: &str = r##"module Stepped
  extend ActiveSupport::Concern

  class ContractBase
    def self.fields
      @fields ||= []
    end

    def self.field(name)
      fields << name
    end

    def page
      3
    end
  end

  class_methods do
    def steps
      @steps ||= []
    end

    def params(name = :default, base_class: ContractBase, &block)
      contract_class = Class.new(base_class).tap { it.class_eval(&block) if block }
      const_set("#{name.to_s.classify.sub("Default", "")}Contract", contract_class)
      steps << [name, contract_class]
    end
  end
end
"##;

const SERVICE: &str = r##"class ListService
  include Stepped

  params do
    field :page

    def limit = 20

    def offset = (page - 1) * limit

    private

    def hidden = 1
  end

  params :filter
end
"##;

const REDEEM: &str = r##"class RedeemService
  include Stepped

  params base_class: ListService::Contract do
    field :code
  end
end
"##;

pub fn overlay() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/concerns/stepped.rb", STEPS)
        .write("app/models/list_service.rb", SERVICE)
        .write("app/models/redeem_service.rb", REDEEM)
}

pub const ASSERTIONS: &str = r##"contract = ListService::Contract
raise contract.name.inspect unless contract.name == "ListService::Contract"
raise contract.superclass.inspect unless contract.superclass == Stepped::ContractBase
raise contract.fields.inspect unless contract.fields == [:page]
raise contract.new.offset.inspect unless contract.new.offset == 40
raise "hidden is public" if contract.new.respond_to?(:hidden)
filter = ListService::FilterContract
raise filter.superclass.inspect unless filter.superclass == Stepped::ContractBase
raise filter.fields.inspect unless filter.fields == []
steps = ListService.steps
raise steps.inspect unless steps.length == 2 && steps[0] == [:default, contract] && steps[1] == [:filter, filter]
redeem = RedeemService::Contract
raise redeem.superclass.inspect unless redeem.superclass == contract
raise redeem.fields.inspect unless redeem.fields == [:code]
raise redeem.new.offset.inspect unless redeem.new.offset == 40
puts "service params contract passed"
"##;
