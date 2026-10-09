# Time-competitive secondary shortcut (measurements only)

Status: not shipped. Follow-up 35 measured three thresholds against
`CORRIDOR_SKELETON_BUILD=9` (the committed constant; referred to as
build 10 in that brief). Keep-all secondary (Pass 6) was reverted
because it changed case a. No product default.

Proposed rule: a chain of secondary road between two nodes already on
the skeleton is kept only if driving it is faster than the major-only
path between the same two nodes by at least a share and a number of
minutes. Drive time uses tagged maxspeed, else the highway fallback
(motorway 100, trunk 80, primary 70, secondary 60 km/h).

The 23 km of secondary near 60.12028, 9.77698 that the faster Elsa line
needs is the ostlandet probe below (edges and km inside a 20 km radius).

## Ostlandet probe (60.12028, 9.77698, 20 km)

| Setting | edges | km | kept |
| --- | ---: | ---: | --- |
| build 9 | 0 | 0.0 | no |
| 20 % and 5 min | 525 | 85.0 | yes |
| 30 % and 10 min | 383 | 57.9 | yes |
| 40 % and 15 min | 383 | 57.9 | yes |

## Gate vs follow-up 33 (host; case e after Gävleborg)

sc30 and sc40 failed the gate on case a wall time only (32.6 s and
31.9 s versus the 25.6 s baseline). Routes a to d matched follow-up 33
except at 20 % / 5 min.

| Setting | a km | a min | a wall | a hops vs FU33 | b | c | d | e |
| --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: |
| follow-up 33 / restore | 1440.723 | 982.2 | 30.8 s | — | 375.330 / 346.1 / 4.5 s | 528.737 / 419.9 / 4.5 s | 1586.813 / 954.6 / 45.4 s | 2348.198 / 1690.8 / 13.7 s |
| 20 % / 5 min | 1436.998 | 979.6 | 32.8 s | hops 1–3 changed (Lübeck / Schleswig-Holstein); 4–18 same. Same as keep-all. | same | same | same | same |
| 30 % / 10 min | 1440.723 | 982.2 | 32.6 s | none | same | same | same | same |
| 40 % / 15 min | 1440.723 | 982.2 | 31.9 s | none | same | same | same | same |

Case a changes only at 20 % / 5 min: those Schleswig-Holstein secondary
chains beat the major path by at least 20 % and 5 min. At 30 % and 40 %
they drop. The Elsa 23 km is kept at all three settings, but the 2348 km
e line does not use it.

## Per region versus build 9

`rule_ms` is the rule itself (sum of tiles). `wall` is the full rebuild.
Peak RSS of the rule itself barely moved (typically 0). Process high
water during rebuild was about 394 MB (20 %), 393 MB (30 %), 397 MB (40 %).
Rebuild wall: 59.8 s, 62.2 s, 51.8 s.

Nodes / edges / bytes at build 9:

| Region | nodes | edges | bytes |
| --- | ---: | ---: | ---: |
| dalarna | 5570 | 13396 | 1103596 |
| denmark | 31518 | 69526 | 5947984 |
| finland | 74607 | 177570 | 15028793 |
| gavleborg | 3919 | 8902 | 759564 |
| halland | 2717 | 6515 | 574962 |
| hamburg | 9772 | 16884 | 1544567 |
| jamtland | 4296 | 10454 | 832395 |
| mecklenburg-vorpommern | 17908 | 44102 | 3712616 |
| niedersachsen | 62204 | 154457 | 13246701 |
| nord-norge | 28800 | 73108 | 6156969 |
| norrbotten | 8761 | 20481 | 1672208 |
| ostlandet | 65365 | 164693 | 14007917 |
| schleswig-holstein | 21649 | 52648 | 4475618 |
| skane | 8105 | 17803 | 1528912 |
| sorlandet | 14108 | 31546 | 2697350 |
| vasterbotten | 6646 | 15805 | 1293921 |
| vasternorrland | 5175 | 12813 | 1066430 |
| vastra_gotaland | 13867 | 35362 | 2990496 |
| vestlandet | 42910 | 103977 | 8792131 |

### 20 % / 5 min

| Region | nodes | edges | bytes | d-nodes | d-edges | d-bytes | wall | rule_ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| dalarna | 5683 | 13622 | 1121768 | +113 | +226 | +18172 | 1.0 s | 5 |
| denmark | 40202 | 86686 | 7464683 | +8684 | +17160 | +1516699 | 4.6 s | 1739 |
| finland | 81721 | 191405 | 16264129 | +7114 | +13835 | +1235336 | 7.9 s | 2284 |
| gavleborg | 4043 | 9148 | 780538 | +124 | +246 | +20974 | 0.4 s | 21 |
| halland | 2822 | 6726 | 592934 | +105 | +211 | +17972 | 1.0 s | 15 |
| hamburg | 10887 | 18902 | 1729737 | +1115 | +2018 | +185170 | 2.2 s | 863 |
| jamtland | 4296 | 10454 | 832395 | 0 | 0 | 0 | 1.1 s | 4 |
| mecklenburg-vorpommern | 24187 | 57136 | 4846143 | +6279 | +13034 | +1133527 | 2.8 s | 684 |
| niedersachsen | 90820 | 213638 | 18649190 | +28616 | +59181 | +5402489 | 13.2 s | 10189 |
| nord-norge | 29734 | 75084 | 6334113 | +934 | +1976 | +177144 | 1.8 s | 54 |
| norrbotten | 9070 | 21078 | 1724273 | +309 | +597 | +52065 | 1.4 s | 29 |
| ostlandet | 75554 | 187117 | 16001303 | +10189 | +22424 | +1993386 | 6.1 s | 3477 |
| schleswig-holstein | 33559 | 78131 | 6775674 | +11910 | +25483 | +2300056 | 5.2 s | 2883 |
| skane | 9658 | 20774 | 1787641 | +1553 | +2971 | +258729 | 1.4 s | 411 |
| sorlandet | 15813 | 35007 | 3008973 | +1705 | +3461 | +311623 | 1.9 s | 158 |
| vasterbotten | 7017 | 16559 | 1356393 | +371 | +754 | +62472 | 1.3 s | 18 |
| vasternorrland | 5668 | 13805 | 1151828 | +493 | +992 | +85398 | 1.1 s | 27 |
| vastra_gotaland | 15206 | 37897 | 3204102 | +1339 | +2535 | +213606 | 2.1 s | 349 |
| vestlandet | 45516 | 109663 | 9299884 | +2606 | +5686 | +507753 | 3.3 s | 484 |

### 30 % / 10 min

| Region | nodes | edges | bytes | d-nodes | d-edges | d-bytes | wall | rule_ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| dalarna | 5570 | 13396 | 1103596 | 0 | 0 | 0 | 1.5 s | 5 |
| denmark | 37033 | 80079 | 6887353 | +5515 | +10553 | +939369 | 6.0 s | 1405 |
| finland | 78335 | 184834 | 15675863 | +3728 | +7264 | +647070 | 11.3 s | 1992 |
| gavleborg | 3988 | 9043 | 770950 | +69 | +141 | +11386 | 0.4 s | 19 |
| halland | 2822 | 6726 | 592934 | +105 | +211 | +17972 | 1.0 s | 15 |
| hamburg | 10088 | 17512 | 1600269 | +316 | +628 | +55702 | 3.3 s | 728 |
| jamtland | 4296 | 10454 | 832395 | 0 | 0 | 0 | 1.1 s | 4 |
| mecklenburg-vorpommern | 21282 | 51014 | 4318911 | +3374 | +6912 | +606295 | 2.7 s | 577 |
| niedersachsen | 76723 | 184644 | 16005435 | +14519 | +30187 | +2758734 | 10.9 s | 7967 |
| nord-norge | 29332 | 74188 | 6254485 | +532 | +1080 | +97516 | 1.7 s | 42 |
| norrbotten | 9070 | 21078 | 1724273 | +309 | +597 | +52065 | 1.3 s | 28 |
| ostlandet | 70312 | 175031 | 14936467 | +4947 | +10338 | +928550 | 4.9 s | 2433 |
| schleswig-holstein | 28843 | 67517 | 5825152 | +7194 | +14869 | +1349534 | 4.9 s | 2559 |
| skane | 8767 | 19058 | 1638631 | +662 | +1255 | +109719 | 1.3 s | 325 |
| sorlandet | 15049 | 33457 | 2869449 | +941 | +1911 | +172099 | 2.0 s | 140 |
| vasterbotten | 7017 | 16559 | 1356393 | +371 | +754 | +62472 | 1.3 s | 18 |
| vasternorrland | 5314 | 13093 | 1090952 | +139 | +280 | +24522 | 1.2 s | 25 |
| vastra_gotaland | 14395 | 36387 | 3078750 | +528 | +1025 | +88254 | 2.2 s | 306 |
| vestlandet | 44065 | 106725 | 9034626 | +1155 | +2748 | +242495 | 3.2 s | 407 |

### 40 % / 15 min

| Region | nodes | edges | bytes | d-nodes | d-edges | d-bytes | wall | rule_ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| dalarna | 5570 | 13396 | 1103596 | 0 | 0 | 0 | 0.9 s | 3 |
| denmark | 33929 | 74172 | 6359953 | +2411 | +4646 | +411969 | 4.0 s | 1232 |
| finland | 76013 | 180412 | 15280997 | +1406 | +2842 | +252204 | 7.4 s | 1849 |
| gavleborg | 3918 | 8901 | 759458 | -1 | -1 | -106 | 0.4 s | 19 |
| halland | 2784 | 6648 | 586442 | +67 | +133 | +11480 | 1.0 s | 17 |
| hamburg | 9908 | 17153 | 1568834 | +136 | +269 | +24267 | 2.0 s | 668 |
| jamtland | 4296 | 10454 | 832395 | 0 | 0 | 0 | 1.1 s | 4 |
| mecklenburg-vorpommern | 19823 | 47977 | 4054814 | +1915 | +3875 | +342198 | 2.7 s | 510 |
| niedersachsen | 69940 | 170736 | 14726019 | +7736 | +16279 | +1479318 | 9.8 s | 6870 |
| nord-norge | 29189 | 73896 | 6228183 | +389 | +788 | +71214 | 1.7 s | 38 |
| norrbotten | 8923 | 20809 | 1698620 | +162 | +328 | +26412 | 1.3 s | 27 |
| ostlandet | 68286 | 170683 | 14548197 | +2921 | +5990 | +540280 | 4.8 s | 2318 |
| schleswig-holstein | 26157 | 61653 | 5300143 | +4508 | +9005 | +824525 | 4.2 s | 1863 |
| skane | 8369 | 18328 | 1572165 | +264 | +525 | +43253 | 1.2 s | 289 |
| sorlandet | 14495 | 32327 | 2767131 | +387 | +781 | +69781 | 1.8 s | 120 |
| vasterbotten | 6865 | 16251 | 1331135 | +219 | +446 | +37214 | 1.3 s | 18 |
| vasternorrland | 5175 | 12813 | 1066430 | 0 | 0 | 0 | 1.1 s | 22 |
| vastra_gotaland | 13953 | 35524 | 3004463 | +86 | +162 | +13967 | 2.0 s | 270 |
| vestlandet | 43586 | 105764 | 8949268 | +676 | +1787 | +157137 | 3.1 s | 372 |

Gävleborg at 40 % was one edge (106 bytes) under the first one-shot
build-9 file. The restore rebuild of all stems together wrote 759458
bytes and left a to e identical to the post-Gävleborg product gate.
