// str: build a string from pieces, split it and replace in it.
const parts = [];
for (let i = 0; i < 100000; i++) {
  parts.push('item' + String(i));
}
const s = parts.join(',');
const words = s.split(',');
const replaced = s.split('item').join('it');
console.log(s.length, words.length, replaced.length, replaced.split('it9').length - 1);
