import assert from 'node:assert/strict';
import test from 'node:test';
import config from '../next.config.mjs';
test('Fumadocs wraps the Next configuration rather than the phase string',()=>{
 assert.equal(typeof config,'object');
 assert.equal(config.reactStrictMode,true);
 assert.equal(Object.keys(config).some(key=>/^\d+$/.test(key)),false);
 assert.equal(typeof config.webpack,'function');
});
