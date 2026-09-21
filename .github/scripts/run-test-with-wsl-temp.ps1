param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string] $TestBinary,

    [Parameter(Position = 1, ValueFromRemainingArguments = $true)]
    [string[]] $TestArguments
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($env:AI_MANAGER_WSL_TEST_TEMP)) {
    throw "AI_MANAGER_WSL_TEST_TEMP must point to the WSL2-backed temporary directory"
}

$env:TEMP = $env:AI_MANAGER_WSL_TEST_TEMP
$env:TMP = $env:AI_MANAGER_WSL_TEST_TEMP

& $TestBinary @TestArguments
exit $LASTEXITCODE
