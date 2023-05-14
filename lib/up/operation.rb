require_relative '../colorize'
require_relative '../utils'


class Operation
  attr_reader :config, :index

  def initialize(config, index: nil)
    @config = config
    @index = index

    check_valid_operation!
  end

  # up is the method that will be called when `omni up`
  # is executed. It should return true if the operation
  # was successful, false in case of errors, and nil if
  # the operation was not applicable
  def up
    raise NotImplementedError
  end

  # down is the method that will be called when `omni down`
  # is executed. It should return true if the operation
  # was successful, false in case of errors, and nil if
  # the operation was not applicable
  def down
    raise NotImplementedError
  end

  private

  def check_valid_operation!
    nil
  end

  def check_params(required_params: nil, allowed_params: nil, check_against: nil)
    check_against = config if check_against.nil?
    required_params ||= []
    allowed_params ||= []
    allowed_params.push(*required_params)

    required_params.each do |key|
      config_error("missing #{key.yellow}") unless check_against[key]
    end

    check_against.each_key do |key|
      config_error("unknown key #{key.yellow}") unless allowed_params.include?(key)
    end
  end

  def config_error(message)
    error("invalid #{'up'.yellow} configuration for "\
          "#{self.class.name.yellow}#{" (idx: #{index.to_s.yellow})" if index}: "\
          "#{message}")
  end

  def had_errors
    @had_errors ||= false
  end

  def run_error(command)
    error("issue while running #{command.yellow}", print_only: true)
    @had_errors = true
  end
end

