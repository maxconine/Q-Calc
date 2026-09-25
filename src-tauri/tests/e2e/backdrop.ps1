# a full-screen backdrop in two alternating bands, for the screenshots of the window's edge, shadow and blur
# colours are hex without the #, which a command line could take for a comment
param([string]$Color = 'f3f3f3', [string]$Alt = 'd0d7e2')

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$form = New-Object System.Windows.Forms.Form
$form.Text = 'qcalc-e2e-backdrop'
$form.FormBorderStyle = 'None'
$form.StartPosition = 'Manual'
$form.ShowInTaskbar = $false
$form.Bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$form.BackColor = [System.Drawing.ColorTranslator]::FromHtml("#$Color")
$band = New-Object System.Drawing.SolidBrush([System.Drawing.ColorTranslator]::FromHtml("#$Alt"))
$form.Add_Paint({
    param($s, $e)
    for ($x = 0; $x -lt $s.Width; $x += 160) { $e.Graphics.FillRectangle($band, $x, 0, 80, $s.Height) }
})
[System.Windows.Forms.Application]::Run($form)
