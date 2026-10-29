cask "q-calc" do
  arch arm: "apple-silicon", intel: "intel"

  version "3.0.0"
  sha256 arm:   "a783a9f677e397b9889cfeb08cb5fd28356a3eb468bcdca36a668956ba01700a",
         intel: "37e9c9d7ca91ccd683916fe02a0bb20c0c213a310be6bb733019de7662d53658"

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
