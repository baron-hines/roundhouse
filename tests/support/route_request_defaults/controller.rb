class DefaultsController < ApplicationController
  def show
    render plain: [params[:text].inspect, params[:symbol].inspect, params[:number].inspect, params[:yes].inspect, params[:no].inspect, params[:absent].inspect].join("|")
  end
end
