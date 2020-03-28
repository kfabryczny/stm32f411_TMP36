## TMP36 temperature sensor

Temperature is read every second from the analog TMP36 sensor and displayed on an OLED display
in Celsius and Fahrenheit degrees. Correctly handles temperatures below zero.

Conversion factor has to be adjusted according to the ADC resolution and Vcc voltage used.

see here for a very good explanation: 
https://learn.adafruit.com/tmp36-temperature-sensor/using-a-temp-sensor

Added a simple circular buffer for ten consecutive readings, that are averaged to get the sample used for voltage to temperature conversion. This to limit the fluctuations of the displayed value. Resolution is also lowered to 10 bits only.

