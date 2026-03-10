package com.twogreensuns.android.network

import com.squareup.moshi.Json
import com.squareup.moshi.JsonClass

@JsonClass(generateAdapter = true)
data class InverterData(
    @Json(name = "p_pv")   val pPv:   Double,
    @Json(name = "p_load") val pLoad: Double,
    @Json(name = "p_grid") val pGrid: Double,
    @Json(name = "p_akku") val pAkku: Double,
    val timestamp: String,
)

@JsonClass(generateAdapter = true)
data class TeslaState(
    val connected: Boolean,
    val charging: Boolean,
    @Json(name = "charging_amps") val chargingAmps: Int,
)

@JsonClass(generateAdapter = true)
data class StatusResponse(
    val inverter: InverterData?,
    val tesla: TeslaState?,
)

@JsonClass(generateAdapter = true)
data class AppConfig(
    val uuid: String,
    @Json(name = "inverter_ipv4_address")    val inverterIpv4Address: String,
    @Json(name = "installation_display_name") val installationDisplayName: String,
    @Json(name = "installation_latitude")    val installationLatitude: Double,
    @Json(name = "installation_longitude")   val installationLongitude: Double,
    @Json(name = "polling_interval_minutes") val pollingIntervalMinutes: Int,
    @Json(name = "tesla_vin")                val teslaVin: String?,
    @Json(name = "tesla_max_amps")           val teslaMaxAmps: Int,
    @Json(name = "http_port")                val httpPort: Int,
    @Json(name = "mdns_hostname")            val mdnsHostname: String,
    @Json(name = "use_mock_ble")             val useMockBle: Boolean,
)

@JsonClass(generateAdapter = true)
data class LogLevelRequest(val level: String)
