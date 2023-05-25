require_relative '../colorize'
require_relative '../utils'
require_relative 'asdf_operation'


class NodeOperation < AsdfOperationTool
  private

  def tool
    'nodejs'
  end
end

class NodejsOperation < NodeOperation; end
