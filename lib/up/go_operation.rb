require_relative '../colorize'
require_relative '../utils'
require_relative 'asdf_operation'


class GoOperation < AsdfOperationTool
  def shadowenv(env)
    file_name = "600_#{tool}_#{tool_version}.lisp"

    contents = <<~LISP
    (provide "#{tool}" "#{tool_version}")

    (when-let ((go-root (env/get "GOROOT")))
      (env/remove-from-pathlist "PATH" (path-concat go-root "bin")))

    (env/set "GOROOT" ())

    (when (null (env/get "OMNI_DATA_HOME"))
      (env/set "OMNI_DATA_HOME"
        (path-concat
          (if (or (null (env/get "XDG_DATA_HOME")) (not (string-prefix-p "/" (env/get "XDG_DATA_HOME"))))
            (path-concat (env/get "HOME") ".local/share")
            (env/get "XDG_DATA_HOME"))
          "omni")))

    (let ((tool_path (path-concat (env/get "OMNI_DATA_HOME") "asdf" "installs" "#{tool}" "#{tool_version}")))
      (do
        (env/set "GOROOT" (path-concat tool_path "go"))
        (env/prepend-to-pathlist "PATH" (path-concat tool_path "go" "bin"))))
    LISP

    env.write(file_name, contents)
  end

  private

  def tool
    'golang'
  end
end

class GolangOperation < GoOperation; end
