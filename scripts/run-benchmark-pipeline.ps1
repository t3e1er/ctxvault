<#
.SYNOPSIS
    Forwarding script to benchmarks/run-benchmark-pipeline.ps1
#>
[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$ForwardArgs
)

$targetScript = Join-Path (Split-Path $PSScriptRoot -Parent) "benchmarks\run-benchmark-pipeline.ps1"
if (-not (Test-Path $targetScript)) {
    throw "Consolidated benchmark script not found at $targetScript"
}

& $targetScript @PSBoundParameters
