class DefaultsController < ApplicationController
  def show
    render plain: [params[:text].inspect, params[:symbol].inspect, params[:number].inspect, params[:yes].inspect, params[:no].inspect, params[:absent].inspect].join("|")
  end
  def links
    render plain: [optional_path(1), optional_path(number: 1), optional_path(2), optional_path(number: 2), optional_path("1"), literal_path(text: "other"), entry_path(9, 1), entry_path(9, 2), catalog_path("en"), catalog_path("fr")].join("|")
  end
  def routing
    show
  end
end
