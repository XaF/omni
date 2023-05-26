require_relative '../colorize'
require_relative '../utils'
require_relative 'asdf_operation'


class RubyOperation < AsdfOperationTool
  def shadowenv(env)
    file_name = "600_#{tool}_#{tool_version}.lisp"

    contents = <<~LISP
    (provide "#{tool}" "#{tool_version}")

    (when-let ((ruby-root (env/get "RUBY_ROOT")))
     (env/remove-from-pathlist "PATH" (path-concat ruby-root "bin"))
     (when-let ((gem-root (env/get "GEM_ROOT")))
       (env/remove-from-pathlist "PATH" (path-concat gem-root "bin")))
     (when-let ((gem-home (env/get "GEM_HOME")))
       (env/remove-from-pathlist "PATH" (path-concat gem-home "bin"))))

    (env/set "GEM_PATH" ())
    (env/set "GEM_HOME" ())
    (env/set "RUBYOPT" ())

    (when (null (env/get "OMNI_DATA_HOME"))
      (env/set "OMNI_DATA_HOME"
        (path-concat
          (if (or (null (env/get "XDG_DATA_HOME")) (not (string-prefix-p "/" (env/get "XDG_DATA_HOME"))))
            (path-concat (env/get "HOME") ".local/share")
            (env/get "XDG_DATA_HOME"))
          "omni")))

    (let ((tool_path (path-concat (env/get "OMNI_DATA_HOME") "asdf" "installs" "#{tool}" "#{tool_version}")))
      (do
        (env/set "RUBY_ROOT" tool_path)
        (env/prepend-to-pathlist "PATH" (path-concat tool_path "bin"))
        (env/set "RUBY_ENGINE" "#{tool}")
        (env/set "RUBY_VERSION" "#{tool_version}")
        (env/set "GEM_ROOT" (path-concat tool_path "lib/#{tool}/gems/#{tool_version_minor}.0"))
        (env/set "GEM_HOME" (env/get "GEM_ROOT"))))

    (when-let ((gem-root (env/get "GEM_ROOT")))
      (env/prepend-to-pathlist "GEM_PATH" gem-root)
      (env/prepend-to-pathlist "PATH" (path-concat gem-root "bin")))
    LISP

    env.write(file_name, contents)
  end

  private

  def tool
    'ruby'
  end
end
