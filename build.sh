export DYLD_LIBRARY_PATH=/Users/blackjack/micromamba/envs/ros_env/lib:/Users/blackjack/Desktop/bevy_robomaster_simulator/install/rm_interfaces/lib:/Users/blackjack/micromamba/envs/ros_env/opt/rviz_ogre_vendor/lib
export ROS_DISTRO=humble;
export AMENT_PREFIX_PATH=/Users/blackjack/Desktop/bevy_robomaster_simulator/install/rm_interfaces:/Users/blackjack/micromamba/envs/ros_env;

export Python_EXECUTABLE=/Users/blackjack/micromamba/envs/ros_env/bin/python
export Python_INCLUDE_DIRS=/Users/blackjack/micromamba/envs/ros_env/include/python3.11
export Python_LIBRARIES=/Users/blackjack/micromamba/envs/ros_env/lib
export NumPy_INCLUDE_DIRS=/Users/blackjack/micromamba/envs/ros_env/lib/python3.11/site-packages/numpy/core/include

/Users/blackjack/micromamba/envs/ros_env/bin/colcon build --cmake-args \
  -DPython_EXECUTABLE=$Python_EXECUTABLE \
  -DPython_INCLUDE_DIRS=$Python_INCLUDE_DIRS \
  -DPython_LIBRARIES=$Python_LIBRARIES \
  -DNumPy_INCLUDE_DIRS=$NumPy_INCLUDE_DIRS
