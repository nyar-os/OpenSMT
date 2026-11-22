/**
 * Build and release configuration for OpenSMT.
 *
 * Pure literals only: `wae` parses this file statically and never executes it.
 * Build order is fixed: native → ts → staging.
 *
 * The package lives under `projects/packages`; the native addon is built from
 * the `open-smt-napi` crate and staged into the per-platform packages, so
 * `@wae/open-smt` resolves the addon without any environment variable.
 */
import { defineConfig } from '@wae/wae';

export default defineConfig({
    native: {
        crate: 'open-smt-napi',
        cdylib: 'open_smt_napi',
        targets: [
            {
                platform: 'win32',
                arch: 'x64',
                triple: 'x86_64-pc-windows-msvc',
                package: '@wae/open-smt-win32-x64',
                nodeFile: 'open-smt.win32-x64.node',
                dir: 'projects/packages/open-smt-win32-x64',
            },
            {
                platform: 'darwin',
                arch: 'x64',
                triple: 'x86_64-apple-darwin',
                package: '@wae/open-smt-darwin-x64',
                nodeFile: 'open-smt.darwin-x64.node',
                dir: 'projects/packages/open-smt-darwin-x64',
            },
            {
                platform: 'darwin',
                arch: 'arm64',
                triple: 'aarch64-apple-darwin',
                package: '@wae/open-smt-darwin-arm64',
                nodeFile: 'open-smt.darwin-arm64.node',
                dir: 'projects/packages/open-smt-darwin-arm64',
            },
            {
                platform: 'linux',
                arch: 'x64',
                triple: 'x86_64-unknown-linux-gnu',
                package: '@wae/open-smt-linux-x64',
                nodeFile: 'open-smt.linux-x64.node',
                dir: 'projects/packages/open-smt-linux-x64',
            },
            {
                platform: 'linux',
                arch: 'arm64',
                triple: 'aarch64-unknown-linux-gnu',
                package: '@wae/open-smt-linux-arm64',
                nodeFile: 'open-smt.linux-arm64.node',
                dir: 'projects/packages/open-smt-linux-arm64',
            },
        ],
    },
    packages: [
        {
            name: '@wae/open-smt-win32-x64',
            dir: 'projects/packages/open-smt-win32-x64',
            os: ['win32'],
            cpu: ['x64'],
            main: 'open-smt.win32-x64.node',
        },
        {
            name: '@wae/open-smt-darwin-x64',
            dir: 'projects/packages/open-smt-darwin-x64',
            os: ['darwin'],
            cpu: ['x64'],
            main: 'open-smt.darwin-x64.node',
        },
        {
            name: '@wae/open-smt-darwin-arm64',
            dir: 'projects/packages/open-smt-darwin-arm64',
            os: ['darwin'],
            cpu: ['arm64'],
            main: 'open-smt.darwin-arm64.node',
        },
        {
            name: '@wae/open-smt-linux-x64',
            dir: 'projects/packages/open-smt-linux-x64',
            os: ['linux'],
            cpu: ['x64'],
            main: 'open-smt.linux-x64.node',
        },
        {
            name: '@wae/open-smt-linux-arm64',
            dir: 'projects/packages/open-smt-linux-arm64',
            os: ['linux'],
            cpu: ['arm64'],
            main: 'open-smt.linux-arm64.node',
        },
    ],
    build: {
        ts: { packages: ['@wae/open-smt'] },
        outputs: ['projects/packages/open-smt/dist/cli.js', 'projects/packages/open-smt/dist/index.js'],
    },
});
