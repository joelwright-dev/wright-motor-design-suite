# reference/city-ev v0.1.0

Build pack for a production volume of 500 vehicle(s).

| | |
|---|---|
| Parts | 27 |
| Pieces per vehicle | 78 |
| Modelled mass | 573.2 kg |
| Fasteners per vehicle | 133 |
| Assembly steps a person carries out | 90 |
| Parts cost per vehicle | 3000 AUD |
| Tooling assumed | 48890 AUD |

## Bill of materials

| Part | Qty | Hands | Material | Each | Total | How it is made | Cost each |
|---|---|---|---|---|---|---|---|
| braking/caliper | 4 | 2 left, 2 right | alu/a356-cast | 3.40 kg | 13.60 kg | purchased | 120 AUD |
| braking/disc | 4 | - | iron/grey-cast-g3000 | 8.04 kg | 32.17 kg | cast-iron | 16 AUD |
| braking/master-cylinder | 1 | - | alu/housing-average | 2.60 kg | 2.60 kg | purchased | 140 AUD |
| chassis/mcds-v1/crossmember | 7 | - | alu/6061-t6 | 1.30 kg | 9.11 kg | tube-cut | - |
| chassis/mcds-v1/rail-box | 2 | - | cfrp/pultruded-ud-t700-epoxy | 3.44 kg | 6.87 kg | pultrusion-cut | - |
| chassis/mcds-v1/rail-box | 2 | - | cfrp/pultruded-ud-t700-epoxy | 5.94 kg | 11.87 kg | pultrusion-cut | - |
| drivetrain/motor/drive-unit | 1 | - | alu/housing-average | 50.00 kg | 50.00 kg | purchased | - |
| energy/battery/pack-modular | 1 | - | polymer/pack-composite-average | 225.00 kg | 225.00 kg | purchased | - |
| steering/column | 1 | - | steel/e355-tube | 5.20 kg | 5.20 kg | purchased | 180 AUD |
| steering/intermediate-shaft | 1 | - | steel/e355-tube | 2.40 kg | 2.40 kg | purchased | 85 AUD |
| steering/rack | 1 | - | alu/housing-average | 6.80 kg | 6.80 kg | purchased | 260 AUD |
| steering/tie-rod | 2 | 1 left, 1 right | steel/4140-bar | 0.95 kg | 1.90 kg | purchased | 48 AUD |
| steering/wheel | 1 | - | polymer/abs | 2.10 kg | 2.10 kg | purchased | 95 AUD |
| suspension/anti-roll/bar | 1 | - | steel/spring-chrome-silicon | 7.95 kg | 7.95 kg | bent-bar | 22 AUD |
| suspension/anti-roll/bar | 1 | - | steel/spring-chrome-silicon | 7.09 kg | 7.09 kg | bent-bar | 22 AUD |
| suspension/arms/lca-wishbone-a | 4 | 2 left, 2 right | steel/e355-tube | 3.35 kg | 13.39 kg | tube-cut-notch-weld | - |
| suspension/arms/lca-wishbone-a | 4 | 2 left, 2 right | steel/e355-tube | 2.85 kg | 11.41 kg | tube-cut-notch-weld | - |
| suspension/brackets/arm-bracket | 8 | 4 left, 4 right | steel/s355-plate | 2.80 kg | 22.44 kg | flat-cut | - |
| suspension/brackets/arm-bracket | 8 | 4 left, 4 right | steel/s355-plate | 2.99 kg | 23.94 kg | flat-cut | - |
| suspension/brackets/damper-tower | 4 | 2 left, 2 right | steel/s355-plate | 3.40 kg | 13.61 kg | flat-cut-and-weld | 38 AUD |
| suspension/dampers/twin-tube | 4 | - | steel/e355-tube | 2.30 kg | 9.20 kg | purchased | 95 AUD |
| suspension/springs/coil | 4 | - | steel/spring-chrome-silicon | 2.00 kg | 8.02 kg | coiled | 9 AUD |
| suspension/uprights/upright-dw | 4 | 2 left, 2 right | alu/a356-cast | 5.64 kg | 22.55 kg | cast-aluminium | 42 AUD |
| wheels/tyre | 2 | - | rubber/tyre-compound | 7.50 kg | 15.00 kg | purchased | 120 AUD |
| wheels/tyre | 2 | - | rubber/tyre-compound | 7.50 kg | 15.00 kg | purchased | 120 AUD |
| wheels/wheel-steel | 2 | - | steel/dc04-sheet | 8.50 kg | 17.00 kg | purchased | 85 AUD |
| wheels/wheel-steel | 2 | - | steel/dc04-sheet | 8.50 kg | 17.00 kg | purchased | 85 AUD |

## Cut list

| Section | Length | Qty | For | Material |
|---|---|---|---|---|
| box 120 x 60 x 6.0 wall | 1100 mm | 2 | chassis/mcds-v1/rail-box (rail) | cfrp/pultruded-ud-t700-epoxy |
| box 120 x 60 x 6.0 wall | 1900 mm | 2 | chassis/mcds-v1/rail-box (rail) | cfrp/pultruded-ud-t700-epoxy |
| round 106 x 11.0 wall | 320 mm | 4 | suspension/springs/coil (coils) | steel/spring-chrome-silicon |
| round 150 x 46.5 wall | 10 mm | 4 | braking/disc (hat_face) | iron/grey-cast-g3000 |
| round 150 x 9.0 wall | 45 mm | 4 | braking/disc (hat_skirt) | iron/grey-cast-g3000 |
| round 16 x 7.2 wall | 306 mm | 2 | steering/tie-rod (rod) | steel/4140-bar |
| round 16 x 7.5 wall | 920 mm | 1 | suspension/anti-roll/bar (torsion) | steel/spring-chrome-silicon |
| round 16 x 7.5 wall | 300 mm | 1 | suspension/anti-roll/bar (arm_left) | steel/spring-chrome-silicon |
| round 16 x 7.5 wall | 300 mm | 1 | suspension/anti-roll/bar (arm_right) | steel/spring-chrome-silicon |
| round 20 x 9.5 wall | 920 mm | 1 | suspension/anti-roll/bar (torsion) | steel/spring-chrome-silicon |
| round 20 x 9.5 wall | 200 mm | 1 | suspension/anti-roll/bar (arm_left) | steel/spring-chrome-silicon |
| round 20 x 9.5 wall | 200 mm | 1 | suspension/anti-roll/bar (arm_right) | steel/spring-chrome-silicon |
| round 22 x 2.5 wall | 500 mm | 1 | steering/intermediate-shaft (shaft) | steel/e355-tube |
| round 24 x 2.5 wall | 260 mm | 4 | suspension/arms/lca-wishbone-a (front_leg) | steel/e355-tube |
| round 24 x 2.5 wall | 260 mm | 4 | suspension/arms/lca-wishbone-a (rear_leg) | steel/e355-tube |
| round 28 x 2.5 wall | 377 mm | 4 | suspension/arms/lca-wishbone-a (front_leg) | steel/e355-tube |
| round 28 x 2.5 wall | 377 mm | 4 | suspension/arms/lca-wishbone-a (rear_leg) | steel/e355-tube |
| round 280 x 45.0 wall | 24 mm | 4 | braking/disc (ring) | iron/grey-cast-g3000 |
| round 370 x 32.0 wall | 32 mm | 1 | steering/wheel (rim) | polymer/abs |
| round 45 x 3.0 wall | 186 mm | 4 | suspension/dampers/twin-tube (body) | steel/e355-tube |
| round 48 x 2.5 wall | 540 mm | 1 | steering/column (support) | steel/e355-tube |
| round 52 x 6.0 wall | 700 mm | 1 | steering/rack (housing) | alu/housing-average |
| round 60 x 3.0 wall | 900 mm | 7 | chassis/mcds-v1/crossmember (tube) | alu/6061-t6 |

25.0 m of section per vehicle, before offcuts.

## Fasteners

| Fastener | Qty | Torque | With | Fitted by |
|---|---|---|---|---|
| bolt M12 10.9 | 4 | 110 Nm | - | you |
| bolt M12 10.9 | 48 | 90 Nm | - | you |
| bolt M12 10.9 | 16 | 85 Nm | nyloc nut | you |
| bolt M12 10.9 | 8 | 80 Nm | nyloc nut | you |
| bolt M12 8.8 | 18 | 90 Nm | flat washer | you |
| bolt M12x1.5 10.9 | 16 | 110 Nm | - | you |
| bolt M8 8.8 | 2 | 25 Nm | nyloc nut | you |
| nut M10 8.8 | 4 | 45 Nm | nyloc nut | you |
| nut M12 10.9 | 2 | 45 Nm | castellated nut | you |
| nut M12 10.9 | 4 | 80 Nm | castellated nut | you |
| nut M14 10.9 | 4 | 110 Nm | castellated nut | you |
| nut M14x1.5 8.8 | 2 | 60 Nm | lock nut | you |
| nut M16 8.8 | 1 | 45 Nm | - | you |
| screw M8 8.8 | 4 | 12 Nm | - | you |

## Assembly

Follow these in order. Each step only asks for parts that are already in front of you.

### Build the front left corner

2. Lay out front left corner lower front bracket to build on.
3. Fit front left corner lower arm to front left corner lower front bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.
4. Fit front left corner lower arm to front left corner lower rear bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.
5. Fit front left corner upright to front left corner lower arm at balljoint. Use 1 x nut M14 10.9, castellated nut. Tighten to 110 Nm.
6. Fit front left corner upright to front left corner upper arm at balljoint. Use 1 x nut M12 10.9, castellated nut. Tighten to 80 Nm.
7. Fit front left corner disc to front left corner upright at hub. Use 1 x screw M8 8.8. Tighten to 12 Nm.
8. Fit front left corner wheel to front left corner disc at wheel. Use 4 x bolt M12x1.5 10.9. Tighten to 110 Nm.
9. Fit front left corner damper to front left corner lower arm at spring. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 80 Nm.
10. front left corner spring comes already fitted to front left corner damper.
11. Fit front left corner damper to front left corner tower at eye. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 80 Nm.
12. Fit front left corner caliper to front left corner upright at caliper. Use 2 x bolt M12 10.9, medium thread locker. Tighten to 90 Nm.
13. front left corner tyre comes already fitted to front left corner wheel.
14. Fit front left corner upper arm to front left corner upper front bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.
15. Fit front left corner upper arm to front left corner upper rear bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.

### Build the front right corner

17. Lay out front right corner lower front bracket to build on.
18. Fit front right corner lower arm to front right corner lower front bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.
19. Fit front right corner lower arm to front right corner lower rear bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.
20. Fit front right corner upright to front right corner lower arm at balljoint. Use 1 x nut M14 10.9, castellated nut. Tighten to 110 Nm.
21. Fit front right corner upright to front right corner upper arm at balljoint. Use 1 x nut M12 10.9, castellated nut. Tighten to 80 Nm.
22. Fit front right corner disc to front right corner upright at hub. Use 1 x screw M8 8.8. Tighten to 12 Nm.
23. Fit front right corner wheel to front right corner disc at wheel. Use 4 x bolt M12x1.5 10.9. Tighten to 110 Nm.
24. Fit front right corner damper to front right corner lower arm at spring. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 80 Nm.
25. front right corner spring comes already fitted to front right corner damper.
26. Fit front right corner damper to front right corner tower at eye. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 80 Nm.
27. Fit front right corner caliper to front right corner upright at caliper. Use 2 x bolt M12 10.9, medium thread locker. Tighten to 90 Nm.
28. front right corner tyre comes already fitted to front right corner wheel.
29. Fit front right corner upper arm to front right corner upper front bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.
30. Fit front right corner upper arm to front right corner upper rear bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.

### Build the rear left corner

32. Lay out rear left corner lower front bracket to build on.
33. Fit rear left corner lower arm to rear left corner lower front bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.
34. Fit rear left corner lower arm to rear left corner lower rear bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.
35. Fit rear left corner upright to rear left corner lower arm at balljoint. Use 1 x nut M14 10.9, castellated nut. Tighten to 110 Nm.
36. Fit rear left corner upright to rear left corner upper arm at balljoint. Use 1 x nut M12 10.9, castellated nut. Tighten to 80 Nm.
37. Fit rear left corner disc to rear left corner upright at hub. Use 1 x screw M8 8.8. Tighten to 12 Nm.
38. Fit rear left corner wheel to rear left corner disc at wheel. Use 4 x bolt M12x1.5 10.9. Tighten to 110 Nm.
39. Fit rear left corner damper to rear left corner lower arm at spring. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 80 Nm.
40. rear left corner spring comes already fitted to rear left corner damper.
41. Fit rear left corner damper to rear left corner tower at eye. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 80 Nm.
42. Fit rear left corner caliper to rear left corner upright at caliper. Use 2 x bolt M12 10.9, medium thread locker. Tighten to 90 Nm.
43. rear left corner tyre comes already fitted to rear left corner wheel.
44. Fit rear left corner upper arm to rear left corner upper front bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.
45. Fit rear left corner upper arm to rear left corner upper rear bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.

### Build the rear right corner

47. Lay out rear right corner lower front bracket to build on.
48. Fit rear right corner lower arm to rear right corner lower front bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.
49. Fit rear right corner lower arm to rear right corner lower rear bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.
50. Fit rear right corner upright to rear right corner lower arm at balljoint. Use 1 x nut M14 10.9, castellated nut. Tighten to 110 Nm.
51. Fit rear right corner upright to rear right corner upper arm at balljoint. Use 1 x nut M12 10.9, castellated nut. Tighten to 80 Nm.
52. Fit rear right corner disc to rear right corner upright at hub. Use 1 x screw M8 8.8. Tighten to 12 Nm.
53. Fit rear right corner wheel to rear right corner disc at wheel. Use 4 x bolt M12x1.5 10.9. Tighten to 110 Nm.
54. Fit rear right corner damper to rear right corner lower arm at spring. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 80 Nm.
55. rear right corner spring comes already fitted to rear right corner damper.
56. Fit rear right corner damper to rear right corner tower at eye. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 80 Nm.
57. Fit rear right corner caliper to rear right corner upright at caliper. Use 2 x bolt M12 10.9, medium thread locker. Tighten to 90 Nm.
58. rear right corner tyre comes already fitted to rear right corner wheel.
59. Fit rear right corner upper arm to rear right corner upper front bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.
60. Fit rear right corner upper arm to rear right corner upper rear bracket at bush. Use 1 x bolt M12 10.9, nyloc nut. Tighten to 85 Nm.

### Put it together

62. Lay out chassis to build on.
63. Fit battery to chassis at station_left_0. Use 1 x bolt M12 8.8, flat washer. Tighten to 90 Nm.
64. Also bolt battery at mount_fr to chassis at station_right_0. It should line up without forcing. Use 1 x bolt M12 8.8, flat washer. Tighten to 90 Nm.
65. Also bolt battery at mount_rl to chassis at station_left_8. It should line up without forcing. Use 1 x bolt M12 8.8, flat washer. Tighten to 90 Nm.
66. Also bolt battery at mount_rr to chassis at station_right_8. It should line up without forcing. Use 1 x bolt M12 8.8, flat washer. Tighten to 90 Nm.
67. Fit drive to chassis at station_left_10. Use 1 x bolt M12 10.9. Tighten to 110 Nm.
68. Also bolt drive at mount_fr to chassis at station_right_10. It should line up without forcing. Use 1 x bolt M12 10.9. Tighten to 110 Nm.
69. Also bolt drive at mount_rl to chassis at station_left_13. It should line up without forcing. Use 1 x bolt M12 10.9. Tighten to 110 Nm.
70. Also bolt drive at mount_rr to chassis at station_right_13. It should line up without forcing. Use 1 x bolt M12 10.9. Tighten to 110 Nm.
71. Fit front left corner to chassis at station_left_-10. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
72. Also bolt front left corner at station_upper_front to chassis at station_left_-9. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
73. Also bolt front left corner at station_upper_rear to chassis at station_left_-7. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
74. Also bolt front left corner at station_lower_rear to chassis at station_left_-6. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
75. Fit front right corner to chassis at station_right_-10. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
76. Also bolt front right corner at station_upper_front to chassis at station_right_-9. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
77. Also bolt front right corner at station_upper_rear to chassis at station_right_-7. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
78. Also bolt front right corner at station_lower_rear to chassis at station_right_-6. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
79. Fit rear left corner to chassis at station_left_14. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
80. Also bolt rear left corner at station_upper_front to chassis at station_left_15. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
81. Also bolt rear left corner at station_upper_rear to chassis at station_left_17. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
82. Also bolt rear left corner at station_lower_rear to chassis at station_left_18. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
83. Fit rear right corner to chassis at station_right_14. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
84. Also bolt rear right corner at station_upper_front to chassis at station_right_15. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
85. Also bolt rear right corner at station_upper_rear to chassis at station_right_17. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
86. Also bolt rear right corner at station_lower_rear to chassis at station_right_18. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
87. Also bolt front left corner at station_tower to chassis at station_left_-8. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
88. Also bolt front right corner at station_tower to chassis at station_right_-8. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
89. Also bolt rear left corner at station_tower to chassis at station_left_16. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
90. Also bolt rear right corner at station_tower to chassis at station_right_16. It should line up without forcing. Use 2 x bolt M12 10.9. Tighten to 90 Nm.
91. Fit rack to chassis at station_left_-5. Use 2 x bolt M12 8.8. Tighten to 90 Nm.
92. Also bolt rack at mount_r to chassis at station_right_-5. It should line up without forcing. Use 2 x bolt M12 8.8. Tighten to 90 Nm.
93. Fit left tie rod to front left corner at steering_arm. Use 1 x nut M12 10.9, castellated nut. Tighten to 45 Nm.
94. Fit right tie rod to front right corner at steering_arm. Use 1 x nut M12 10.9, castellated nut. Tighten to 45 Nm.
95. Fit intermediate shaft to rack at pinion. Use 1 x bolt M8 8.8, nyloc nut. Tighten to 25 Nm.
96. Fit column to intermediate shaft at upper. Use 1 x bolt M8 8.8, nyloc nut. Tighten to 25 Nm.
97. Fit steering wheel to column at wheel. Use 1 x nut M16 8.8. Tighten to 45 Nm.
98. Fit front arb to chassis at station_left_-11. Use 2 x bolt M12 8.8. Tighten to 90 Nm.
99. Also bolt front arb at mount_r to chassis at station_right_-11. It should line up without forcing. Use 2 x bolt M12 8.8. Tighten to 90 Nm.
100. Also bolt front arb at link_l to front left corner at antiroll. It should line up without forcing. Use 1 x nut M10 8.8, nyloc nut. Tighten to 45 Nm.
101. Also bolt front arb at link_r to front right corner at antiroll. It should line up without forcing. Use 1 x nut M10 8.8, nyloc nut. Tighten to 45 Nm.
102. Fit rear arb to chassis at station_left_12. Use 2 x bolt M12 8.8. Tighten to 90 Nm.
103. Also bolt rear arb at mount_r to chassis at station_right_12. It should line up without forcing. Use 2 x bolt M12 8.8. Tighten to 90 Nm.
104. Also bolt rear arb at link_l to rear left corner at antiroll. It should line up without forcing. Use 1 x nut M10 8.8, nyloc nut. Tighten to 45 Nm.
105. Also bolt rear arb at link_r to rear right corner at antiroll. It should line up without forcing. Use 1 x nut M10 8.8, nyloc nut. Tighten to 45 Nm.
106. Fit master cylinder to chassis at station_left_-4. Use 2 x bolt M12 8.8. Tighten to 90 Nm.
107. Also bolt left tie rod at inner to rack at end_l. It should line up without forcing. Use 1 x nut M14x1.5 8.8, lock nut. Tighten to 60 Nm.
108. Also bolt right tie rod at inner to rack at end_r. It should line up without forcing. Use 1 x nut M14x1.5 8.8, lock nut. Tighten to 60 Nm.

This pack is generated from the model. It is not a substitute for a qualified inspection of the finished vehicle.
