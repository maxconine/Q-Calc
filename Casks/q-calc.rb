cask "q-calc" do
  arch arm: "apple-silicon", intel: "intel"

  version "2.0.7"
  sha256 arm:   "d9a7d1f61709baa7bab318785baeedda06bf41635c29d6a588c6b8517eae829f",
         intel: "7b45bd644366e928c352431b362bb1cf704c1bbc433dfe381fd111306f522749"

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
