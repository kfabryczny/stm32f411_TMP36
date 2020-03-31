## TMP36 temperature sensor

Temperature is read every second from the analog TMP36 sensor and displayed on an OLED display
in Celsius and Fahrenheit degrees. Correctly handles temperatures below zero.

Conversion factor has to be adjusted according to the ADC resolution and Vcc voltage used.

see here for a very good explanation: 
https://learn.adafruit.com/tmp36-temperature-sensor/using-a-temp-sensor

Improved following the STMicroelectronics Application Note AN4073 "How to improve ADC accuracy".

"Business logic": get an accurate reading by sampling 12 times and dropping the two biggest and two smallest values,
then averaging the remaining 8.

"Presentation logic": to make the displayed value more stable for the user, average the last 8 temperature values.