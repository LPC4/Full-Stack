param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CargoArgs
)

$env:UPDATE = "1"
& cargo test @CargoArgs

