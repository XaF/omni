require_relative '../colorize'
require_relative '../utils'
require_relative 'asdf_operation'


class PythonOperation < AsdfOperationTool
  private

  def tool
    'python'
  end
end
