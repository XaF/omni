require_relative '../colorize'
require_relative '../utils'
require_relative 'asdf_operation'


class RustOperation < AsdfOperationTool
  private

  def tool
    'rust'
  end
end
