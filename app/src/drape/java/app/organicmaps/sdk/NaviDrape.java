package app.organicmaps.sdk;

/**
 * Navi entry points into the CoMaps drape (liborganicmaps.so, patches/comaps/0004).
 *
 * Lives in the CoMaps SDK package so the JNI symbols resolve; compiled into the Navi app, not
 * the pinned SDK checkout. Every overlay call must run on the main (UI) thread: the CoMaps
 * BookmarkManager is single-threaded.
 */
public final class NaviDrape
{
  private NaviDrape() {}

  // Camera.
  public static native void nativeSetCamera(double lat, double lon, double level, double bearingDeg, boolean anim);
  /** lat, lon, level, bearingDeg, pitchDeg, widthPx, heightPx, my-position mode (LocationState constants). */
  public static native double[] nativeGetCamera();
  public static native void nativeScaleBy(double factor, boolean anim);
  public static native void nativeRotateTo(double bearingDeg, boolean anim);
  public static native void nativeFitRect(double minLat, double minLon, double maxLat, double maxLon, boolean anim);
  /** 0 = free, 1 = follow, 2 = follow and rotate (heading up). */
  public static native void nativeSetFollowMode(int mode);
  public static native void nativeSetNavigation(boolean on, boolean pedestrian);
  public static native void nativeBlockTapEvents(boolean block);
  public static native double[] nativeScreenToLatLon(float x, float y);
  public static native void nativeRegisterMaps();
  public static native void nativeSetSetting(String key, String value);

  // Overlays.
  public static native long nativeCreateGroup(String name);
  public static native void nativeSetMarks(long group, double[] latlon, String[] names, int[] icons, int[] colors);
  public static native void nativeSetTracks(long group, int[] counts, double[] latlon, int[] argb, float[] widths);
  public static native void nativeAddSubroute(double[] latlon, int type, int[] turnIdx);
  public static native void nativeClearRoute();
  public static native void nativeSetRoutePoints(double[] latlon, String[] titles);
}
