# CLAUDE.md

This project is a highly efficient and fast clone of Wispr Flow
The Pipeline goes like this:
    - User Press and hold a preassigned button to speak
    - We capture the users current focused app contents and users voice
    - Once the user releases the button, the audio along with the major vocabulary in the screen is send to wisper turbo for transcription
    - Once the audio is transcribed, we run the audio through a highspeed llm to remove filler words and make the text proper.
    - The generated text is pasted to the users currently focused text area as soon as it arrives.

The Major challenges we had are assigning the global key and monitoring it in background, capturing the audio, reading the screen through asseseblity apis, sending only the relevant vocabulary as initial prompt for the wisper model, getting the response back and fixing it using llm, pasting in realtime without delay.
slave.mK