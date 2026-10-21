cask "q-calc" do
  arch arm: "apple-silicon", intel: "intel"

  version "2.0.14"
  sha256 arm:   "d104fd3490698b496ebb61a3608e8bc6c67b4133e07c2f429a4b4256950b6b1f",
         intel: "03a8bb6bad23b6723cddc81d668493ecf9fb0eb778e21387d2d8350414e282a9"

  url "https://github.com/maxconine/Q-Calc/releases/download/v#{version}/Q-Calc-#{version}-#{arch}.zip"
  name "Q Calc"
  desc "Spotlight-style scientific calculator"
  homepage "https://github.com/maxconine/Q-Calc"

  livecheck do
    url :homepage
    strategy :github_latest
  end

  auto_updates true
  depends_on macos: ">= :ventura"

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
