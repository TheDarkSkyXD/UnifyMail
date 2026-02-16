@echo off
pushd "%~dp0"
node ..\scripts\mock-mailsync.js %*
popd
