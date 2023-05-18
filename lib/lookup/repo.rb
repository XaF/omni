require_relative '../colorize'
require_relative '../env'
require_relative '../omniorg'
require_relative '../utils'


class LookupRepo
  def self.lookup(repo)
    # If the parameter starts with `.` or `/`, we can
    # assume it is a path, and we can just try to cd
    # to it
    return repo if repo && repo.start_with?('/', '.', '~/') || repo == '-'

    # Try to find the repository by directly looking for
    # it in our known paths
    naive_lookup = LookupRepo.basic_naive_lookup(repo)
    return naive_lookup if naive_lookup

    # Try to find the repository by looking for it in
    # the file system, under the main git directory,
    # using find
    begin
      LookupRepo.file_system_lookup(repo)
    rescue UserInteraction::StoppedByUserError
      exit 0
    rescue UserInteraction::NoMatchError
      nil
    end
  end

  def self.autocomplete(repo)
    # If the repo starts with '.' or '/', the completion should
    # be path completion and not repo completion
    if repo && repo.start_with?('.', '/', '~/') || repo == '-'
      (Dir.glob("#{repo}*/") + Dir.glob("#{repo}*/**/*/")).sort.each do |match|
        puts match
      end unless repo == '-'

      exit 0
    end

    # We can try and fetch all the repositories, or part of repository
    # paths, that could start with the value provided so far
    match_repo = Regexp.new(%r{(^|/)(?<match>#{Regexp.escape(repo || '')}.*)$})

    potential_matches = []
    OmniOrgs.repos(dedup: false) do |dir, path, dir_path|
      # Trim prefix from dir_path
      rel_path = if dir_path.start_with?("#{OmniEnv::OMNI_GIT}/")
        dir_path[OmniEnv::OMNI_GIT.length + 1..-1]
      else
        dir_path
      end

      match = match_repo.match(rel_path)
      next unless match

      potential_matches << [
        dir_path,
        match[:match],
      ]
    end

    if potential_matches&.any?
      # Filter and order the potential matches
      potential_matches.uniq! { |dir_path, _| dir_path }
      potential_matches.map! { |_, path| path }
      potential_matches.uniq!
      potential_matches.sort!

      # Write the potential matches if we find any
      potential_matches
        .each { |path| puts path }
    end
  end

  def self.path_match_skip_prompt_if
    Config.dig('cd', 'path_match_skip_prompt_if') || stringify_keys({
      first_min: 0.80,
      second_max: 0.40,
    })
  end

  def self.basic_naive_lookup(repo)
    paths = OmniOrgs.map { |org| org.path?(repo) }
    paths << OmniEnv::OMNI_GIT unless repo

    paths.compact!
    paths.uniq!

    paths.each do |path|
      next unless File.directory?(path)

      return path
    end

    nil
  end

  def self.file_system_lookup(repo)
    return unless repo

    split_repo = repo.split('/')

    progress_bar = TTYProgressBar.new(
      "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".yellow} Searching for repository [:bar]",
      bar_format: :triangle,
      clear: true,
      output: STDERR,
    )

    starting = Time.now
    begin
      potential_matches = []
      OmniOrgs.repos(dedup: false) do |dir, path, dir_path|
        progress_bar.advance

        expected_match = dir_path.split('/')[-split_repo.length..-1]

        if expected_match == split_repo
          # Show a tip if the search took more than a second
          STDERR.puts "#{"omni:".light_cyan} #{"Did you know?".bold} A proper #{"OMNI_ORG".yellow} environment variable will make repository lookup much faster." if Time.now - starting > 1
          return dir_path
        end

        potential_matches << dir_path
      end
    ensure
      progress_bar.finish
      progress_bar.stop
    end

    # Exit if we did not find any potential matches
    return if potential_matches.empty?

    # If we got here and we did not find an exact match,
    # try offering a did-you-mean suggestion
    UserInteraction.did_you_mean?(
      potential_matches.uniq, repo,
      skip_with_score: self.path_match_skip_prompt_if,
    )
  end
end
