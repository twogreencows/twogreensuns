package com.twogreensuns.android.network

import com.squareup.moshi.Moshi
import com.squareup.moshi.kotlin.reflect.KotlinJsonAdapterFactory
import retrofit2.Retrofit
import retrofit2.converter.moshi.MoshiConverterFactory
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.PUT
import retrofit2.http.Path

interface TwoGreenSunsApi {

    @GET("api/v1/status")
    suspend fun status(): StatusResponse

    @GET("api/v1/config")
    suspend fun getConfig(): AppConfig

    @PUT("api/v1/config")
    suspend fun putConfig(@Body config: AppConfig)

    @PUT("api/v1/log/level")
    suspend fun setLogLevel(@Body body: LogLevelRequest)

    @POST("api/v1/tesla/discover")
    suspend fun teslaDiscover()

    @POST("api/v1/tesla/charge/start")
    suspend fun teslaStart()

    @POST("api/v1/tesla/charge/stop")
    suspend fun teslaStop()

    @POST("api/v1/tesla/charge/amps/{n}")
    suspend fun teslaSetAmps(@Path("n") amps: Int)
}

object ApiClient {
    private val moshi = Moshi.Builder()
        .addLast(KotlinJsonAdapterFactory())
        .build()

    fun create(baseUrl: String): TwoGreenSunsApi =
        Retrofit.Builder()
            .baseUrl(baseUrl)
            .addConverterFactory(MoshiConverterFactory.create(moshi))
            .build()
            .create(TwoGreenSunsApi::class.java)
}
