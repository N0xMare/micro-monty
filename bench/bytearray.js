// bytearray: a byte array built by push, another by appending a chunk with
// item writes, then index reads, a stepped copy and a sum (an Array of
// numbers stands in for bytearray).
const buf = [];
for (let i = 0; i < 600000; i++) {
  buf.push((i * 7) & 255);
}
const chunk = [97, 98, 99, 100, 101, 102, 103, 104];
const grown = [];
for (let i = 0; i < 80000; i++) {
  grown.push(...chunk);
  grown[grown.length - 1] = i & 127;
}
let total = 0;
for (let i = 0; i < buf.length; i += 3) {
  total += buf[i];
}
let stepped = 0;
for (let i = 0; i < grown.length; i += 7) {
  stepped += grown[i];
}
console.log(buf.length, total, grown.length, stepped);
