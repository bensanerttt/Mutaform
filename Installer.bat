@echo off
REM Installerer Mutaform (VST3 + CLAP). Maa kjores som administrator.
net session >nul 2>&1
if errorlevel 1 (
  echo Hoyreklikk paa Installer.bat og velg "Kjor som administrator".
  pause
  exit /b 1
)
set SRC=%~dp0
xcopy /E /I /Y "%SRC%Mutaform.vst3" "%CommonProgramFiles%\VST3\Mutaform.vst3"
if not exist "%CommonProgramFiles%\CLAP" mkdir "%CommonProgramFiles%\CLAP"
copy /Y "%SRC%Mutaform.clap" "%CommonProgramFiles%\CLAP\Mutaform.clap"
echo.
echo Ferdig! Aapne FL Studio og kjor Plugin Manager - Find more plugins.
pause
