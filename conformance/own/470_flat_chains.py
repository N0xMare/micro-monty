# Flat chains compile at any length, as on CPython: operators, attribute,
# subscript and call chains, conditional expressions chained in their else,
# elif arms, f-string fields, string concatenation and stacked decorators.
# Only real nesting counts toward the 200-level limit (docs/user-guide.md).
a = 3
x = a + 1 - 2 * 3 // 4 + 5 - 6 * 7 // 1 + 2 - 3 * 4 // 5 + 6 - 7 * 1 // 2 + 3 - 4 * 5 // 6 + 7 - 1 * 2 // 3 + 4 - 5 * 6 // 7 + 1 - 2 * 3 // 4 + 5 - 6 * 7 // 1 + 2 - 3 * 4 // 5 + 6 - 7 * 1 // 2 + 3 - 4 * 5 // 6 + 7 - 1 * 2 // 3 + 4 - 5 * 6 // 7 + 1 - 2 * 3 // 4 + 5 - 6 * 7 // 1 + 2 - 3 * 4 // 5 + 6 - 7 * 1 // 2 + 3 - 4 * 5 // 6 + 7 - 1 * 2 // 3 + 4 - 5 * 6 // 7 + 1 - 2 * 3 // 4 + 5 - 6 * 7 // 1 + 2 - 3 * 4 // 5 + 6 - 7 * 1 // 2 + 3 - 4 * 5 // 6 + 7 - 1 * 2 // 3 + 4 - 5 * 6 // 7 + 1 - 2 * 3 // 4 + 5 - 6 * 7 // 1 + 2 - 3 * 4 // 5 + 6 - 7 * 1 // 2 + 3 - 4 * 5 // 6 + 7 - 1 * 2 // 3 + 4 - 5 * 6 // 7 + 1 - 2 * 3 // 4 + 5 - 6 * 7 // 1 + 2 - 3 * 4 // 5 + 6 - 7 * 1 // 2 + 3 - 4 * 5 // 6 + 7 - 1 * 2 // 3 + 4 - 5 * 6 // 7 + 1 - 2 * 3 // 4 + 5 - 6 * 7 // 1 + 2 - 3 * 4 // 5 + 6 - 7 * 1 // 2 + 3 - 4 * 5 // 6 + 7 - 1 * 2 // 3 + 4 - 5 * 6 // 7 + 1 - 2 * 3 // 4 + 5 - 6 * 7 // 1 + 2 - 3 * 4 // 5 + 6 - 7 * 1 // 2 + 3 - 4 * 5 // 6 + 7 - 1 * 2 // 3 + 4 - 5 * 6 // 7 + 1 - 2 * 3 // 4 + 5 - 6 * 7 // 1 + 2 - 3 * 4 // 5 + 6 - 7 * 1 // 2 + 3 - 4 * 5 // 6 + 7 - 1 * 2 // 3 + 4 - 5 * 6 // 7 + 1 - 2 * 3 // 4 + 5 - 6 * 7 // 1 + 2 - 3 * 4 // 5 + 6 - 7 * 1 // 2 + 3 - 4 * 5 // 6 + 7 - 1 * 2 // 3 + 4 - 5 * 6 // 7 + 1 - 2 * 3 // 4 + 5 - 6 * 7 // 1 + 2 - 3 * 4 // 5 + 6 - 7 * 1 // 2 + 3 - 4 * 5 // 6
print('operators', x)
print('concat', len('ab' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd' 'cd'))


class B:
    def __init__(self):
        self.n = 0
        self.me = self
        self.items = [self]

    def add(self, k):
        self.n += k
        return self


b = B().add(0).add(1).add(2).add(3).add(4).add(5).add(6).add(7).add(8).add(9).add(10).add(11).add(12).add(13).add(14).add(15).add(16).add(17).add(18).add(19).add(20).add(21).add(22).add(23).add(24).add(25).add(26).add(27).add(28).add(29).add(30).add(31).add(32).add(33).add(34).add(35).add(36).add(37).add(38).add(39).add(40).add(41).add(42).add(43).add(44).add(45).add(46).add(47).add(48).add(49).add(50).add(51).add(52).add(53).add(54).add(55).add(56).add(57).add(58).add(59).add(60).add(61).add(62).add(63).add(64).add(65).add(66).add(67).add(68).add(69).add(70).add(71).add(72).add(73).add(74).add(75).add(76).add(77).add(78).add(79).add(80).add(81).add(82).add(83).add(84).add(85).add(86).add(87).add(88).add(89).add(90).add(91).add(92).add(93).add(94).add(95).add(96).add(97).add(98).add(99).add(100).add(101).add(102).add(103).add(104).add(105).add(106).add(107).add(108).add(109).add(110).add(111).add(112).add(113).add(114).add(115).add(116).add(117).add(118).add(119).add(120).add(121).add(122).add(123).add(124).add(125).add(126).add(127).add(128).add(129).add(130).add(131).add(132).add(133).add(134).add(135).add(136).add(137).add(138).add(139).add(140).add(141).add(142).add(143).add(144).add(145).add(146).add(147).add(148).add(149).add(150).add(151).add(152).add(153).add(154).add(155).add(156).add(157).add(158).add(159).add(160).add(161).add(162).add(163).add(164).add(165).add(166).add(167).add(168).add(169).add(170).add(171).add(172).add(173).add(174).add(175).add(176).add(177).add(178).add(179).add(180).add(181).add(182).add(183).add(184).add(185).add(186).add(187).add(188).add(189).add(190).add(191).add(192).add(193).add(194).add(195).add(196).add(197).add(198).add(199).add(200).add(201).add(202).add(203).add(204).add(205).add(206).add(207).add(208).add(209).add(210).add(211).add(212).add(213).add(214).add(215).add(216).add(217).add(218).add(219).add(220).add(221).add(222).add(223).add(224).add(225).add(226).add(227).add(228).add(229).add(230).add(231).add(232).add(233).add(234).add(235).add(236).add(237).add(238).add(239).add(240).add(241).add(242).add(243).add(244).add(245).add(246).add(247).add(248).add(249)
print('calls', b.n)
print('attributes', b.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me.me is b)
print('subscripts', b.items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].items[0].me.n)
print('mixed', B().add(1).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).me.items[-1].add(2).n)


def pick(x):
    return 'v0' if x == 0 else 'v1' if x == 1 else 'v2' if x == 2 else 'v3' if x == 3 else 'v4' if x == 4 else 'v5' if x == 5 else 'v6' if x == 6 else 'v7' if x == 7 else 'v8' if x == 8 else 'v9' if x == 9 else 'v10' if x == 10 else 'v11' if x == 11 else 'v12' if x == 12 else 'v13' if x == 13 else 'v14' if x == 14 else 'v15' if x == 15 else 'v16' if x == 16 else 'v17' if x == 17 else 'v18' if x == 18 else 'v19' if x == 19 else 'v20' if x == 20 else 'v21' if x == 21 else 'v22' if x == 22 else 'v23' if x == 23 else 'v24' if x == 24 else 'v25' if x == 25 else 'v26' if x == 26 else 'v27' if x == 27 else 'v28' if x == 28 else 'v29' if x == 29 else 'v30' if x == 30 else 'v31' if x == 31 else 'v32' if x == 32 else 'v33' if x == 33 else 'v34' if x == 34 else 'v35' if x == 35 else 'v36' if x == 36 else 'v37' if x == 37 else 'v38' if x == 38 else 'v39' if x == 39 else 'v40' if x == 40 else 'v41' if x == 41 else 'v42' if x == 42 else 'v43' if x == 43 else 'v44' if x == 44 else 'v45' if x == 45 else 'v46' if x == 46 else 'v47' if x == 47 else 'v48' if x == 48 else 'v49' if x == 49 else 'v50' if x == 50 else 'v51' if x == 51 else 'v52' if x == 52 else 'v53' if x == 53 else 'v54' if x == 54 else 'v55' if x == 55 else 'v56' if x == 56 else 'v57' if x == 57 else 'v58' if x == 58 else 'v59' if x == 59 else 'v60' if x == 60 else 'v61' if x == 61 else 'v62' if x == 62 else 'v63' if x == 63 else 'v64' if x == 64 else 'v65' if x == 65 else 'v66' if x == 66 else 'v67' if x == 67 else 'v68' if x == 68 else 'v69' if x == 69 else 'v70' if x == 70 else 'v71' if x == 71 else 'v72' if x == 72 else 'v73' if x == 73 else 'v74' if x == 74 else 'v75' if x == 75 else 'v76' if x == 76 else 'v77' if x == 77 else 'v78' if x == 78 else 'v79' if x == 79 else 'v80' if x == 80 else 'v81' if x == 81 else 'v82' if x == 82 else 'v83' if x == 83 else 'v84' if x == 84 else 'v85' if x == 85 else 'v86' if x == 86 else 'v87' if x == 87 else 'v88' if x == 88 else 'v89' if x == 89 else 'v90' if x == 90 else 'v91' if x == 91 else 'v92' if x == 92 else 'v93' if x == 93 else 'v94' if x == 94 else 'v95' if x == 95 else 'v96' if x == 96 else 'v97' if x == 97 else 'v98' if x == 98 else 'v99' if x == 99 else 'v100' if x == 100 else 'v101' if x == 101 else 'v102' if x == 102 else 'v103' if x == 103 else 'v104' if x == 104 else 'v105' if x == 105 else 'v106' if x == 106 else 'v107' if x == 107 else 'v108' if x == 108 else 'v109' if x == 109 else 'v110' if x == 110 else 'v111' if x == 111 else 'v112' if x == 112 else 'v113' if x == 113 else 'v114' if x == 114 else 'v115' if x == 115 else 'v116' if x == 116 else 'v117' if x == 117 else 'v118' if x == 118 else 'v119' if x == 119 else 'v120' if x == 120 else 'v121' if x == 121 else 'v122' if x == 122 else 'v123' if x == 123 else 'v124' if x == 124 else 'v125' if x == 125 else 'v126' if x == 126 else 'v127' if x == 127 else 'v128' if x == 128 else 'v129' if x == 129 else 'v130' if x == 130 else 'v131' if x == 131 else 'v132' if x == 132 else 'v133' if x == 133 else 'v134' if x == 134 else 'v135' if x == 135 else 'v136' if x == 136 else 'v137' if x == 137 else 'v138' if x == 138 else 'v139' if x == 139 else 'v140' if x == 140 else 'v141' if x == 141 else 'v142' if x == 142 else 'v143' if x == 143 else 'v144' if x == 144 else 'v145' if x == 145 else 'v146' if x == 146 else 'v147' if x == 147 else 'v148' if x == 148 else 'v149' if x == 149 else 'v150' if x == 150 else 'v151' if x == 151 else 'v152' if x == 152 else 'v153' if x == 153 else 'v154' if x == 154 else 'v155' if x == 155 else 'v156' if x == 156 else 'v157' if x == 157 else 'v158' if x == 158 else 'v159' if x == 159 else 'v160' if x == 160 else 'v161' if x == 161 else 'v162' if x == 162 else 'v163' if x == 163 else 'v164' if x == 164 else 'v165' if x == 165 else 'v166' if x == 166 else 'v167' if x == 167 else 'v168' if x == 168 else 'v169' if x == 169 else 'v170' if x == 170 else 'v171' if x == 171 else 'v172' if x == 172 else 'v173' if x == 173 else 'v174' if x == 174 else 'v175' if x == 175 else 'v176' if x == 176 else 'v177' if x == 177 else 'v178' if x == 178 else 'v179' if x == 179 else 'v180' if x == 180 else 'v181' if x == 181 else 'v182' if x == 182 else 'v183' if x == 183 else 'v184' if x == 184 else 'v185' if x == 185 else 'v186' if x == 186 else 'v187' if x == 187 else 'v188' if x == 188 else 'v189' if x == 189 else 'v190' if x == 190 else 'v191' if x == 191 else 'v192' if x == 192 else 'v193' if x == 193 else 'v194' if x == 194 else 'v195' if x == 195 else 'v196' if x == 196 else 'v197' if x == 197 else 'v198' if x == 198 else 'v199' if x == 199 else 'v200' if x == 200 else 'v201' if x == 201 else 'v202' if x == 202 else 'v203' if x == 203 else 'v204' if x == 204 else 'v205' if x == 205 else 'v206' if x == 206 else 'v207' if x == 207 else 'v208' if x == 208 else 'v209' if x == 209 else 'v210' if x == 210 else 'v211' if x == 211 else 'v212' if x == 212 else 'v213' if x == 213 else 'v214' if x == 214 else 'v215' if x == 215 else 'v216' if x == 216 else 'v217' if x == 217 else 'v218' if x == 218 else 'v219' if x == 219 else 'v220' if x == 220 else 'v221' if x == 221 else 'v222' if x == 222 else 'v223' if x == 223 else 'v224' if x == 224 else 'v225' if x == 225 else 'v226' if x == 226 else 'v227' if x == 227 else 'v228' if x == 228 else 'v229' if x == 229 else 'v230' if x == 230 else 'v231' if x == 231 else 'v232' if x == 232 else 'v233' if x == 233 else 'v234' if x == 234 else 'v235' if x == 235 else 'v236' if x == 236 else 'v237' if x == 237 else 'v238' if x == 238 else 'v239' if x == 239 else 'v240' if x == 240 else 'v241' if x == 241 else 'v242' if x == 242 else 'v243' if x == 243 else 'v244' if x == 244 else 'v245' if x == 245 else 'v246' if x == 246 else 'v247' if x == 247 else 'v248' if x == 248 else 'v249' if x == 249 else 'none'


print('conditional', pick(0), pick(137), pick(249), pick(-1))


def arm(x):
    if x == 0:
        return 'arm0'
    elif x == 1:
        return 'arm1'
    elif x == 2:
        return 'arm2'
    elif x == 3:
        return 'arm3'
    elif x == 4:
        return 'arm4'
    elif x == 5:
        return 'arm5'
    elif x == 6:
        return 'arm6'
    elif x == 7:
        return 'arm7'
    elif x == 8:
        return 'arm8'
    elif x == 9:
        return 'arm9'
    elif x == 10:
        return 'arm10'
    elif x == 11:
        return 'arm11'
    elif x == 12:
        return 'arm12'
    elif x == 13:
        return 'arm13'
    elif x == 14:
        return 'arm14'
    elif x == 15:
        return 'arm15'
    elif x == 16:
        return 'arm16'
    elif x == 17:
        return 'arm17'
    elif x == 18:
        return 'arm18'
    elif x == 19:
        return 'arm19'
    elif x == 20:
        return 'arm20'
    elif x == 21:
        return 'arm21'
    elif x == 22:
        return 'arm22'
    elif x == 23:
        return 'arm23'
    elif x == 24:
        return 'arm24'
    elif x == 25:
        return 'arm25'
    elif x == 26:
        return 'arm26'
    elif x == 27:
        return 'arm27'
    elif x == 28:
        return 'arm28'
    elif x == 29:
        return 'arm29'
    elif x == 30:
        return 'arm30'
    elif x == 31:
        return 'arm31'
    elif x == 32:
        return 'arm32'
    elif x == 33:
        return 'arm33'
    elif x == 34:
        return 'arm34'
    elif x == 35:
        return 'arm35'
    elif x == 36:
        return 'arm36'
    elif x == 37:
        return 'arm37'
    elif x == 38:
        return 'arm38'
    elif x == 39:
        return 'arm39'
    elif x == 40:
        return 'arm40'
    elif x == 41:
        return 'arm41'
    elif x == 42:
        return 'arm42'
    elif x == 43:
        return 'arm43'
    elif x == 44:
        return 'arm44'
    elif x == 45:
        return 'arm45'
    elif x == 46:
        return 'arm46'
    elif x == 47:
        return 'arm47'
    elif x == 48:
        return 'arm48'
    elif x == 49:
        return 'arm49'
    elif x == 50:
        return 'arm50'
    elif x == 51:
        return 'arm51'
    elif x == 52:
        return 'arm52'
    elif x == 53:
        return 'arm53'
    elif x == 54:
        return 'arm54'
    elif x == 55:
        return 'arm55'
    elif x == 56:
        return 'arm56'
    elif x == 57:
        return 'arm57'
    elif x == 58:
        return 'arm58'
    elif x == 59:
        return 'arm59'
    elif x == 60:
        return 'arm60'
    elif x == 61:
        return 'arm61'
    elif x == 62:
        return 'arm62'
    elif x == 63:
        return 'arm63'
    elif x == 64:
        return 'arm64'
    elif x == 65:
        return 'arm65'
    elif x == 66:
        return 'arm66'
    elif x == 67:
        return 'arm67'
    elif x == 68:
        return 'arm68'
    elif x == 69:
        return 'arm69'
    elif x == 70:
        return 'arm70'
    elif x == 71:
        return 'arm71'
    elif x == 72:
        return 'arm72'
    elif x == 73:
        return 'arm73'
    elif x == 74:
        return 'arm74'
    elif x == 75:
        return 'arm75'
    elif x == 76:
        return 'arm76'
    elif x == 77:
        return 'arm77'
    elif x == 78:
        return 'arm78'
    elif x == 79:
        return 'arm79'
    elif x == 80:
        return 'arm80'
    elif x == 81:
        return 'arm81'
    elif x == 82:
        return 'arm82'
    elif x == 83:
        return 'arm83'
    elif x == 84:
        return 'arm84'
    elif x == 85:
        return 'arm85'
    elif x == 86:
        return 'arm86'
    elif x == 87:
        return 'arm87'
    elif x == 88:
        return 'arm88'
    elif x == 89:
        return 'arm89'
    elif x == 90:
        return 'arm90'
    elif x == 91:
        return 'arm91'
    elif x == 92:
        return 'arm92'
    elif x == 93:
        return 'arm93'
    elif x == 94:
        return 'arm94'
    elif x == 95:
        return 'arm95'
    elif x == 96:
        return 'arm96'
    elif x == 97:
        return 'arm97'
    elif x == 98:
        return 'arm98'
    elif x == 99:
        return 'arm99'
    elif x == 100:
        return 'arm100'
    elif x == 101:
        return 'arm101'
    elif x == 102:
        return 'arm102'
    elif x == 103:
        return 'arm103'
    elif x == 104:
        return 'arm104'
    elif x == 105:
        return 'arm105'
    elif x == 106:
        return 'arm106'
    elif x == 107:
        return 'arm107'
    elif x == 108:
        return 'arm108'
    elif x == 109:
        return 'arm109'
    elif x == 110:
        return 'arm110'
    elif x == 111:
        return 'arm111'
    elif x == 112:
        return 'arm112'
    elif x == 113:
        return 'arm113'
    elif x == 114:
        return 'arm114'
    elif x == 115:
        return 'arm115'
    elif x == 116:
        return 'arm116'
    elif x == 117:
        return 'arm117'
    elif x == 118:
        return 'arm118'
    elif x == 119:
        return 'arm119'
    elif x == 120:
        return 'arm120'
    elif x == 121:
        return 'arm121'
    elif x == 122:
        return 'arm122'
    elif x == 123:
        return 'arm123'
    elif x == 124:
        return 'arm124'
    elif x == 125:
        return 'arm125'
    elif x == 126:
        return 'arm126'
    elif x == 127:
        return 'arm127'
    elif x == 128:
        return 'arm128'
    elif x == 129:
        return 'arm129'
    elif x == 130:
        return 'arm130'
    elif x == 131:
        return 'arm131'
    elif x == 132:
        return 'arm132'
    elif x == 133:
        return 'arm133'
    elif x == 134:
        return 'arm134'
    elif x == 135:
        return 'arm135'
    elif x == 136:
        return 'arm136'
    elif x == 137:
        return 'arm137'
    elif x == 138:
        return 'arm138'
    elif x == 139:
        return 'arm139'
    elif x == 140:
        return 'arm140'
    elif x == 141:
        return 'arm141'
    elif x == 142:
        return 'arm142'
    elif x == 143:
        return 'arm143'
    elif x == 144:
        return 'arm144'
    elif x == 145:
        return 'arm145'
    elif x == 146:
        return 'arm146'
    elif x == 147:
        return 'arm147'
    elif x == 148:
        return 'arm148'
    elif x == 149:
        return 'arm149'
    elif x == 150:
        return 'arm150'
    elif x == 151:
        return 'arm151'
    elif x == 152:
        return 'arm152'
    elif x == 153:
        return 'arm153'
    elif x == 154:
        return 'arm154'
    elif x == 155:
        return 'arm155'
    elif x == 156:
        return 'arm156'
    elif x == 157:
        return 'arm157'
    elif x == 158:
        return 'arm158'
    elif x == 159:
        return 'arm159'
    elif x == 160:
        return 'arm160'
    elif x == 161:
        return 'arm161'
    elif x == 162:
        return 'arm162'
    elif x == 163:
        return 'arm163'
    elif x == 164:
        return 'arm164'
    elif x == 165:
        return 'arm165'
    elif x == 166:
        return 'arm166'
    elif x == 167:
        return 'arm167'
    elif x == 168:
        return 'arm168'
    elif x == 169:
        return 'arm169'
    elif x == 170:
        return 'arm170'
    elif x == 171:
        return 'arm171'
    elif x == 172:
        return 'arm172'
    elif x == 173:
        return 'arm173'
    elif x == 174:
        return 'arm174'
    elif x == 175:
        return 'arm175'
    elif x == 176:
        return 'arm176'
    elif x == 177:
        return 'arm177'
    elif x == 178:
        return 'arm178'
    elif x == 179:
        return 'arm179'
    elif x == 180:
        return 'arm180'
    elif x == 181:
        return 'arm181'
    elif x == 182:
        return 'arm182'
    elif x == 183:
        return 'arm183'
    elif x == 184:
        return 'arm184'
    elif x == 185:
        return 'arm185'
    elif x == 186:
        return 'arm186'
    elif x == 187:
        return 'arm187'
    elif x == 188:
        return 'arm188'
    elif x == 189:
        return 'arm189'
    elif x == 190:
        return 'arm190'
    elif x == 191:
        return 'arm191'
    elif x == 192:
        return 'arm192'
    elif x == 193:
        return 'arm193'
    elif x == 194:
        return 'arm194'
    elif x == 195:
        return 'arm195'
    elif x == 196:
        return 'arm196'
    elif x == 197:
        return 'arm197'
    elif x == 198:
        return 'arm198'
    elif x == 199:
        return 'arm199'
    elif x == 200:
        return 'arm200'
    elif x == 201:
        return 'arm201'
    elif x == 202:
        return 'arm202'
    elif x == 203:
        return 'arm203'
    elif x == 204:
        return 'arm204'
    elif x == 205:
        return 'arm205'
    elif x == 206:
        return 'arm206'
    elif x == 207:
        return 'arm207'
    elif x == 208:
        return 'arm208'
    elif x == 209:
        return 'arm209'
    elif x == 210:
        return 'arm210'
    elif x == 211:
        return 'arm211'
    elif x == 212:
        return 'arm212'
    elif x == 213:
        return 'arm213'
    elif x == 214:
        return 'arm214'
    elif x == 215:
        return 'arm215'
    elif x == 216:
        return 'arm216'
    elif x == 217:
        return 'arm217'
    elif x == 218:
        return 'arm218'
    elif x == 219:
        return 'arm219'
    elif x == 220:
        return 'arm220'
    elif x == 221:
        return 'arm221'
    elif x == 222:
        return 'arm222'
    elif x == 223:
        return 'arm223'
    elif x == 224:
        return 'arm224'
    elif x == 225:
        return 'arm225'
    elif x == 226:
        return 'arm226'
    elif x == 227:
        return 'arm227'
    elif x == 228:
        return 'arm228'
    elif x == 229:
        return 'arm229'
    elif x == 230:
        return 'arm230'
    elif x == 231:
        return 'arm231'
    elif x == 232:
        return 'arm232'
    elif x == 233:
        return 'arm233'
    elif x == 234:
        return 'arm234'
    elif x == 235:
        return 'arm235'
    elif x == 236:
        return 'arm236'
    elif x == 237:
        return 'arm237'
    elif x == 238:
        return 'arm238'
    elif x == 239:
        return 'arm239'
    elif x == 240:
        return 'arm240'
    elif x == 241:
        return 'arm241'
    elif x == 242:
        return 'arm242'
    elif x == 243:
        return 'arm243'
    elif x == 244:
        return 'arm244'
    elif x == 245:
        return 'arm245'
    elif x == 246:
        return 'arm246'
    elif x == 247:
        return 'arm247'
    elif x == 248:
        return 'arm248'
    elif x == 249:
        return 'arm249'
    else:
        return 'else'


print('elif', arm(0), arm(200), arm(249), arm(250))
s = f'{a + 0:02d},{a}{a + 2:02d},{a}{a + 4:02d},{a}{a + 6:02d},{a}{a + 8:02d},{a}{a + 10:02d},{a}{a + 12:02d},{a}{a + 14:02d},{a}{a + 16:02d},{a}{a + 18:02d},{a}{a + 20:02d},{a}{a + 22:02d},{a}{a + 24:02d},{a}{a + 26:02d},{a}{a + 28:02d},{a}{a + 30:02d},{a}{a + 32:02d},{a}{a + 34:02d},{a}{a + 36:02d},{a}{a + 38:02d},{a}{a + 40:02d},{a}{a + 42:02d},{a}{a + 44:02d},{a}{a + 46:02d},{a}{a + 48:02d},{a}{a + 50:02d},{a}{a + 52:02d},{a}{a + 54:02d},{a}{a + 56:02d},{a}{a + 58:02d},{a}{a + 60:02d},{a}{a + 62:02d},{a}{a + 64:02d},{a}{a + 66:02d},{a}{a + 68:02d},{a}{a + 70:02d},{a}{a + 72:02d},{a}{a + 74:02d},{a}{a + 76:02d},{a}{a + 78:02d},{a}{a + 80:02d},{a}{a + 82:02d},{a}{a + 84:02d},{a}{a + 86:02d},{a}{a + 88:02d},{a}{a + 90:02d},{a}{a + 92:02d},{a}{a + 94:02d},{a}{a + 96:02d},{a}{a + 98:02d},{a}{a + 100:02d},{a}{a + 102:02d},{a}{a + 104:02d},{a}{a + 106:02d},{a}{a + 108:02d},{a}{a + 110:02d},{a}{a + 112:02d},{a}{a + 114:02d},{a}{a + 116:02d},{a}{a + 118:02d},{a}{a + 120:02d},{a}{a + 122:02d},{a}{a + 124:02d},{a}{a + 126:02d},{a}{a + 128:02d},{a}{a + 130:02d},{a}{a + 132:02d},{a}{a + 134:02d},{a}{a + 136:02d},{a}{a + 138:02d},{a}{a + 140:02d},{a}{a + 142:02d},{a}{a + 144:02d},{a}{a + 146:02d},{a}{a + 148:02d},{a}{a + 150:02d},{a}{a + 152:02d},{a}{a + 154:02d},{a}{a + 156:02d},{a}{a + 158:02d},{a}{a + 160:02d},{a}{a + 162:02d},{a}{a + 164:02d},{a}{a + 166:02d},{a}{a + 168:02d},{a}{a + 170:02d},{a}{a + 172:02d},{a}{a + 174:02d},{a}{a + 176:02d},{a}{a + 178:02d},{a}{a + 180:02d},{a}{a + 182:02d},{a}{a + 184:02d},{a}{a + 186:02d},{a}{a + 188:02d},{a}{a + 190:02d},{a}{a + 192:02d},{a}{a + 194:02d},{a}{a + 196:02d},{a}{a + 198:02d},{a}{a + 200:02d},{a}{a + 202:02d},{a}{a + 204:02d},{a}{a + 206:02d},{a}{a + 208:02d},{a}{a + 210:02d},{a}{a + 212:02d},{a}{a + 214:02d},{a}{a + 216:02d},{a}{a + 218:02d},{a}{a + 220:02d},{a}{a + 222:02d},{a}{a + 224:02d},{a}{a + 226:02d},{a}{a + 228:02d},{a}{a + 230:02d},{a}{a + 232:02d},{a}{a + 234:02d},{a}{a + 236:02d},{a}{a + 238:02d},{a}{a + 240:02d},{a}{a + 242:02d},{a}{a + 244:02d},{a}{a + 246:02d},{a}{a + 248:02d},{a}{a + 250:02d},{a}{a + 252:02d},{a}{a + 254:02d},{a}{a + 256:02d},{a}{a + 258:02d},{a}{a + 260:02d},{a}{a + 262:02d},{a}{a + 264:02d},{a}{a + 266:02d},{a}{a + 268:02d},{a}{a + 270:02d},{a}{a + 272:02d},{a}{a + 274:02d},{a}{a + 276:02d},{a}{a + 278:02d},{a}{a + 280:02d},{a}{a + 282:02d},{a}{a + 284:02d},{a}{a + 286:02d},{a}{a + 288:02d},{a}{a + 290:02d},{a}{a + 292:02d},{a}{a + 294:02d},{a}{a + 296:02d},{a}{a + 298:02d},{a}'
print('f-string', len(s), s[:24], s[-12:])
print('compare', 0 < 1 < 2 < 3 < 4 < 5 < 6 < 7 < 8 < 9 < 10 < 11 < 12 < 13 < 14 < 15 < 16 < 17 < 18 < 19 < 20 < 21 < 22 < 23 < 24 < 25 < 26 < 27 < 28 < 29 < 30 < 31 < 32 < 33 < 34 < 35 < 36 < 37 < 38 < 39 < 40 < 41 < 42 < 43 < 44 < 45 < 46 < 47 < 48 < 49 < 50 < 51 < 52 < 53 < 54 < 55 < 56 < 57 < 58 < 59 < 60 < 61 < 62 < 63 < 64 < 65 < 66 < 67 < 68 < 69 < 70 < 71 < 72 < 73 < 74 < 75 < 76 < 77 < 78 < 79 < 80 < 81 < 82 < 83 < 84 < 85 < 86 < 87 < 88 < 89 < 90 < 91 < 92 < 93 < 94 < 95 < 96 < 97 < 98 < 99 < 100 < 101 < 102 < 103 < 104 < 105 < 106 < 107 < 108 < 109 < 110 < 111 < 112 < 113 < 114 < 115 < 116 < 117 < 118 < 119 < 120 < 121 < 122 < 123 < 124 < 125 < 126 < 127 < 128 < 129 < 130 < 131 < 132 < 133 < 134 < 135 < 136 < 137 < 138 < 139 < 140 < 141 < 142 < 143 < 144 < 145 < 146 < 147 < 148 < 149 < 150 < 151 < 152 < 153 < 154 < 155 < 156 < 157 < 158 < 159 < 160 < 161 < 162 < 163 < 164 < 165 < 166 < 167 < 168 < 169 < 170 < 171 < 172 < 173 < 174 < 175 < 176 < 177 < 178 < 179 < 180 < 181 < 182 < 183 < 184 < 185 < 186 < 187 < 188 < 189 < 190 < 191 < 192 < 193 < 194 < 195 < 196 < 197 < 198 < 199 < 200 < 201 < 202 < 203 < 204 < 205 < 206 < 207 < 208 < 209 < 210 < 211 < 212 < 213 < 214 < 215 < 216 < 217 < 218 < 219 < 220 < 221 < 222 < 223 < 224 < 225 < 226 < 227 < 228 < 229 < 230 < 231 < 232 < 233 < 234 < 235 < 236 < 237 < 238 < 239 < 240 < 241 < 242 < 243 < 244 < 245 < 246 < 247 < 248 < 249, 1 < 1 < 1)


def inc(f):
    return lambda: f() + 1


@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
@inc
def zero():
    return 0


print('decorators', zero())
print('in call', sum([a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a, 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1]))
