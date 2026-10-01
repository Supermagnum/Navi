package no.navi.app

/** Fixture JSON for [CampingPhase5bScreenshotTest]. */
object CampingSuggestFixtures {
    private fun cardJson(
        lat: Double,
        lon: Double,
        country: String,
        fire: String?,
        tier: String = "a",
        notes: String = "[]",
        walkM: String? = null,
        seed: String? = null,
        decline: String? = null,
        legal: String = "Test legal basis",
    ): String {
        val fireField = fire?.let { """"fire_text":"$it",""" } ?: ""
        val walkField = walkM?.let { """"walk_m":$it,""" } ?: ""
        val seedField = seed?.let { """"seed_road_highway":"$it",""" } ?: ""
        val declineField = decline?.let { """"decline":"$it",""" } ?: ""
        return """
            {
              "lat":$lat,"lon":$lon,"accepted":true,"tier":"$tier","country_iso":"$country",
              $declineField"legal_basis":"$legal","sources":["https://example.test/source"],
              $fireField$walkField$seedField
              "notes":$notes,
              "not_checked":{"protected_area":true,"landcover":false},
              "disclaimer":"${CAMPING_PLUGIN_DISCLAIMER.replace("\"", "\\\"")}",
              "location_id":"fix:$lat:$lon"
            }
            """.trimIndent()
    }

    fun hikingJulyFireBan(): String =
        resultJson(
            list = listOf(cardJson(61.12, 10.47, "no", "Open fire ban 15 Apr–15 Sep (Oslo local date).")),
        )

    fun hikingOctoberOutsideBan(): String =
        resultJson(
            list =
                listOf(
                    cardJson(
                        61.12,
                        10.47,
                        "no",
                        "Check local fire rules — outside the usual 15 Apr–15 Sep window.",
                    ),
                ),
        )

    fun mobileHomeVehicleEmptyOnFoot(): String =
        """
        {
          "accepted":0,"rejected":0,"vehicle_accepted":0,"on_foot_accepted":1,
          "disclaimer":"${CAMPING_PLUGIN_DISCLAIMER.replace("\"", "\\\"")}",
          "list":{"cards":[],"seeds_considered":0,"probes_accepted":0,"probes_rejected":0,"disclaimer":""},
          "vehicle":{"cards":[],"seeds_considered":2,"probes_accepted":0,"probes_rejected":0,"disclaimer":""},
          "on_foot_from_here":{
            "cards":[
              ${cardJson(
            61.13,
            10.50,
            "no",
            null,
            notes = """["Park on track; walk in ~420 m to tent spot"]""",
            walkM = "420",
            seed = "track",
        )}
            ],
            "seeds_considered":1,"probes_accepted":1,"probes_rejected":0,"disclaimer":""
          }
        }
        """.trimIndent()

    fun swedishTierASafetyDefault(): String =
        resultJson(
            list =
                listOf(
                    cardJson(
                        59.88,
                        12.19,
                        "se",
                        null,
                        tier = "a",
                        notes =
                            """["Building distance uses Navi safety default — not Swedish law"]""",
                        legal = "Swedish allemansrätt (Tier A)",
                    ),
                ),
        )

    fun svalbardDecline(): String =
        resultJson(
            list =
                listOf(
                    cardJson(
                        78.22,
                        15.63,
                        "sj",
                        null,
                        tier = "d",
                        decline = "svalbard",
                        notes = """["Polar bear deterrent required","Sysselmesteren notification"]""",
                        legal = "Svalbard — no wild camping suggestion",
                    ),
                ),
        )

    private fun resultJson(list: List<String>): String =
        """
        {
          "accepted":${list.size},"rejected":0,
          "disclaimer":"${CAMPING_PLUGIN_DISCLAIMER.replace("\"", "\\\"")}",
          "list":{"cards":[${list.joinToString(",")}],"seeds_considered":1,"probes_accepted":${list.size},"probes_rejected":0,"disclaimer":""},
          "vehicle":{"cards":[],"seeds_considered":0,"probes_accepted":0,"probes_rejected":0,"disclaimer":""},
          "on_foot_from_here":{"cards":[],"seeds_considered":0,"probes_accepted":0,"probes_rejected":0,"disclaimer":""}
        }
        """.trimIndent()
}
