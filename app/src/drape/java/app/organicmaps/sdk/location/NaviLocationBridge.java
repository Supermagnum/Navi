package app.organicmaps.sdk.location;

/** Gives Navi access to the package-private native location entry of {@link LocationState}. */
public final class NaviLocationBridge
{
  private NaviLocationBridge() {}

  public static void push(long timeMs, double lat, double lon, float accuracy, double altitude, float speed,
                          float bearing)
  {
    LocationState.nativeLocationUpdated(timeMs, lat, lon, accuracy, altitude, speed, bearing);
  }

  public static void gpsOff()
  {
    LocationState.nativeOnLocationError(LocationState.ERROR_GPS_OFF);
  }
}
