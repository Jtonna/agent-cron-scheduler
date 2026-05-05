$ErrorActionPreference = "Stop"
$weather = curl.exe -s "https://wttr.in/Fresno,CA?format=%l+%C+%t+%w+%h"
if ([string]::IsNullOrWhiteSpace($weather)) {
    Write-Error "Empty weather response from wttr.in"
    exit 1
}
[Console]::Out.Write($weather.Trim())
