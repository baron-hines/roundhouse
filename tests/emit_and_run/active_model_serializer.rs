//! An `ActiveModel::Serializer` is expanded into plain methods and
//! renders as JSON without the gem, byte for byte as
//! active_model_serializers 0.10.16 does.

use super::active_model_serializer_app as active_model_serializer;

/// `render json: x, serializer: S` and `each_serializer:` answer through
/// the serializer's generated `as_json_str`, with no
/// active_model_serializers gem and no runtime encoder in the emitted
/// controller. The expected text is the gem's under the attributes
/// adapter: declaration order, `key:`, a block, a serializer method, and
/// `if:` / `unless:`.
#[test]
fn an_active_model_serializer_renders_without_the_gem() {
    active_model_serializer::overlay()
        .run_ruby(&format!("{}{}", r#"
require_relative "app/controllers/article_json_controller"
source = File.read("app/controllers/article_json_controller.rb")
raise "the runtime encoder is still reached:\n#{source}" if source.include?("JsonRender")
"#, active_model_serializer::ASSERTIONS))
        .assert_passes();
}
