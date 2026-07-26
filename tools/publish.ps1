# Run this script from the repository root: .\tools\publish.ps1
Write-Host "Cleaning up previous builds..."
if (Test-Path "dist") { Remove-Item -Recurse -Force "dist" }
if (Test-Path "build") { Remove-Item -Recurse -Force "build" }
if (Test-Path "chord_romanizer.egg-info") { Remove-Item -Recurse -Force "chord_romanizer.egg-info" }

Write-Host "Building package..."
python setup.py sdist bdist_wheel
if ($LASTEXITCODE -ne 0) {
    Write-Error "Build failed."
    exit $LASTEXITCODE
}

Write-Host "Checking package with twine..."
twine check dist/*
if ($LASTEXITCODE -ne 0) {
    Write-Error "Twine check failed."
    exit $LASTEXITCODE
}

Write-Host "Uploading to PyPI..."

# Check if .pypirc exists
if (Test-Path "$env:USERPROFILE\.pypirc") {
    $choice = Read-Host "Stored credentials found in .pypirc. Do you want to use them? (Y/n)"
    if ($choice -eq 'n' -or $choice -eq 'N') {
        $token = Read-Host "Enter your PyPI API Token (pypi-xxxxxxxx...)" -AsSecureString
        # Convert SecureString to PlainText for environment variable (Twine needs plain text)
        $BSTR = [System.Runtime.InteropServices.Marshal]::SecureStringToBSTR($token)
        $PlainText = [System.Runtime.InteropServices.Marshal]::PtrToStringAuto($BSTR)
        
        $env:TWINE_USERNAME = "__token__"
        $env:TWINE_PASSWORD = $PlainText
        
        Write-Host "Using provided token."
    } else {
        Write-Host "Using stored credentials from .pypirc."
    }
} else {
    Write-Host "No .pypirc found."
    Write-Host "You will be asked for your username (__token__) and password (API token)." -ForegroundColor Yellow
}

twine upload dist/*
