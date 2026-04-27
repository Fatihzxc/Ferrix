# Listen on the host's USB-UART (CH340 -> COM3) at 115200 8N1 and stream
# every byte to the console as it arrives. Ctrl-C to stop.
#
# This is the "watch the Zynq talk" half of the hardware demo.
# Pair with scripts/load-and-run.ps1 in another shell.
#
# Usage:
#   .\scripts\serial-listen.ps1             # default COM3 @ 115200
#   .\scripts\serial-listen.ps1 COM4 9600   # override port + baud

param(
    [string]$Port = "COM3",
    [int]$Baud = 115200
)

$sp = New-Object System.IO.Ports.SerialPort $Port, $Baud, 'None', 8, 'One'
# Latin-1 (ISO-8859-1) is a 1:1 byte->char mapping, so high-bit bytes
# survive intact instead of being mangled to '?' by ASCIIEncoding.
$sp.Encoding = [System.Text.Encoding]::GetEncoding(28591)
$sp.ReadTimeout = 100

try {
    $sp.Open()
    Write-Host "[$Port @ $Baud baud] listening; Ctrl-C to stop." -ForegroundColor DarkGray
    while ($true) {
        $chunk = $sp.ReadExisting()
        if ($chunk) { [Console]::Write($chunk) }
        Start-Sleep -Milliseconds 50
    }
} finally {
    if ($sp.IsOpen) { $sp.Close() }
}
