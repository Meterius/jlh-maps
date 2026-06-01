build-demo:
    just --justfile crates/jlh_maps_app/justfile --working-directory crates/jlh_maps_app build-release
    npm --prefix packages/jlh_maps run build:demo
    node -e "const fs = require('node:fs'); fs.rmSync('demo', { recursive: true, force: true }); fs.cpSync('packages/jlh_maps/dist', 'demo', { recursive: true });"
