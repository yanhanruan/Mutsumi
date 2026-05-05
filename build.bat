@echo off
chcp 65001 > nul
cd /d "%~dp0"

echo ============================================
echo   Mutsumi Desktop Pet -- Build Script
echo ============================================
echo.

echo [Step 1/2]  Installing / updating dependencies...
pip install -r requirements.txt
if errorlevel 1 (
    echo ERROR: pip install failed. Please check your Python environment.
    pause & exit /b 1
)

echo.
echo [Step 2/2]  Building executable with PyInstaller...
pyinstaller ^
    --onefile ^
    --windowed ^
    --name "MutsumiPet" ^
    --add-data "assets;assets" ^
    --paths "src" ^
    --hidden-import "PIL._tkinter_finder" ^
    src/main.py

if errorlevel 1 (
    echo ERROR: PyInstaller build failed.
    pause & exit /b 1
)

echo.
echo ============================================
echo   Build complete!
echo   Output: dist\MutsumiPet.exe
echo.
echo   TIP: Copy the assets\ folder next to the
echo        .exe if you want to use custom art.
echo ============================================
pause
