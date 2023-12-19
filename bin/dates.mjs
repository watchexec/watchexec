#!/usr/bin/env node

const id = Math.floor(Math.random() * 100);
let n = 0;
const m = 5;
while (n < m) {
	n += 1;
	console.log(`[${id} : ${n}/${m}] ${new Date}`);
	await new Promise(done => setTimeout(done, 2000));
}
