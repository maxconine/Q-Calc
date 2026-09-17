cask "q-calc" do
  version "2.0.0"
  sha256 "b238baefd75b594342c4c0d55da0d1a51411c183f05311f8d5e63a44a7f47d59"

  url "https://github.com/maxconine/Instant-Calculator/releases/download/v#{version}/Q-Calc-#{version}.zip"
  name "Q Calc"
  desc "Spotlight-style scientific calculator"
  homepage "https://github.com/maxconine/Instant-Calculator"

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
  end

  uninstall quit: "com.maxconine.qcalc"

  zap trash: "~/Library/Preferences/com.maxconine.qcalc.plist"

  caveats <<~EOS
    Q Calc is ad-hoc signed. Install with --no-quarantine to skip Gatekeeper,
    or if macOS refuses to open it:

      xattr -cr "/Applications/Q Calc.app"
      open "/Applications/Q Calc.app"

    Field Macs can pick up new versions with:

      brew upgrade --cask q-calc
  EOS
end
