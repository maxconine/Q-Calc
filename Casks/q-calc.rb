cask "q-calc" do
  arch arm: "apple-silicon", intel: "intel"

  version "2.0.15"
  sha256 arm:   "71122d10662848812180f8c54796c386bd411e7b7c4d75b2ea91791682c2eee5",
         intel: "dbceb76ed6b96ec55767501afab828d92d9f417e64d7aca627064aab60e48da6"

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
