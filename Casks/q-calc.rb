cask "q-calc" do
  version "2.0.1"
  sha256 "32235ffc156f2dfab9bfb8d508e07d389fd01a596334365325be86ff809815ae"

  url "https://github.com/maxconine/Q-Calc/releases/download/v#{version}/Q-Calc-#{version}.zip"
  name "Q Calc"
  desc "Spotlight-style scientific calculator"
  homepage "https://github.com/maxconine/Q-Calc"

  livecheck do
    url :homepage
    strategy :github_latest
  end

  depends_on arch: :arm64
  depends_on macos: :sonoma

  app "Q Calc.app"

  postflight_steps do
    if_path_exists "Instant Solver.app", base: :appdir do
      remove "Instant Solver.app", recursive: true, base: :appdir
    end
    run "/usr/bin/xattr", args: ["-cr", "{{appdir}}/Q Calc.app"], must_succeed: false
  end

  uninstall quit: "com.maxconine.qcalc"

  zap trash: "~/Library/Preferences/com.maxconine.qcalc.plist"

  caveats <<~EOS
    Q Calc is a menu-bar app. Press Control + Option + Space to open it.

    Turn on daily Homebrew upgrades for Q Calc:

      zsh "$(brew --prefix)/Library/Taps/maxconine/homebrew-qcalc/macos/enable-autoupdate.sh"
  EOS
end
