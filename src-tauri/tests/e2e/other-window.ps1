# a plain window for the smoke test to click, standing in for "some other app"
param([int]$X = 20, [int]$Y = 400)

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$form = New-Object System.Windows.Forms.Form
$form.Text = 'qcalc-e2e-other'
$form.StartPosition = 'Manual'
$form.Location = New-Object System.Drawing.Point($X, $Y)
$form.Size = New-Object System.Drawing.Size(320, 200)
$form.BackColor = [System.Drawing.Color]::LightSteelBlue
[System.Windows.Forms.Application]::Run($form)
